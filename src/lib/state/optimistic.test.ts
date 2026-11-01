// Optimistic edits put back after a failed save, to what the store holds, and never over a newer
// edit (#358).
import { describe, expect, it, vi } from "vitest";
import { SaveLedger, withEntry } from "./optimistic";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(),
}));
import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { setOverride } from "../api";

type Cat = { category: string; subcategory: string | null };
const cat = (category: string): Cat => ({ category, subcategory: null });

describe("SaveLedger", () => {
  it("a failed save goes back to what the store holds", () => {
    const l = new SaveLedger<Cat>();
    l.load([["A", cat("Tools")]]);
    const t = l.begin("A");
    expect(l.failed("A", t)).toEqual({ value: cat("Tools") });
  });

  it("a key the store doesn't hold goes back to no entry", () => {
    const l = new SaveLedger<Cat>();
    const t = l.begin("A");
    expect(l.failed("A", t)).toEqual({ value: undefined });
  });

  it("leaves a newer edit alone: the older edit's failure changes nothing", () => {
    const l = new SaveLedger<Cat>();
    l.load([["A", cat("Tools")]]);
    const older = l.begin("A");
    l.begin("A");
    expect(l.failed("A", older)).toBeNull();
  });

  it("two failed saves in a row end on the stored value, not on the first failed one", () => {
    const l = new SaveLedger<Cat>();
    l.load([["A", cat("Tools")]]);
    const b = l.begin("A"); // pick Trucks
    const c = l.begin("A"); // pick Cars before it lands
    expect(l.failed("A", b)).toBeNull();
    expect(l.failed("A", c)).toEqual({ value: cat("Tools") });
  });

  it("a save that worked is what a later failure goes back to", () => {
    const l = new SaveLedger<Cat>();
    l.load([["A", cat("Tools")]]);
    l.begin("A");
    l.saved("A", cat("Trucks"));
    const c = l.begin("A");
    expect(l.failed("A", c)).toEqual({ value: cat("Trucks") });
  });

  it("tells two removals apart: reset, pick, reset with the first reset failing", () => {
    const l = new SaveLedger<Cat>();
    l.load([["A", cat("Tools")]]);
    const reset1 = l.begin("A");
    l.begin("A"); // pick Trucks
    const reset2 = l.begin("A");
    expect(l.failed("A", reset1)).toBeNull(); // the newer reset stays on screen
    l.saved("A", cat("Trucks")); // the pick worked
    expect(l.failed("A", reset2)).toEqual({ value: cat("Trucks") }); // the second reset failed
  });

  it("a removal that worked is what a later failure goes back to", () => {
    const l = new SaveLedger<Cat>();
    l.load([["A", cat("Tools")]]);
    l.begin("A");
    l.saved("A", undefined); // reset worked
    const pick = l.begin("A");
    expect(l.failed("A", pick)).toEqual({ value: undefined });
  });

  it("keys don't affect each other", () => {
    const l = new SaveLedger<Cat>();
    const a = l.begin("A");
    l.begin("B");
    expect(l.failed("A", a)).toEqual({ value: undefined });
  });
});

describe("withEntry", () => {
  it("sets or removes one entry in a copy", () => {
    const map = { A: cat("Tools"), B: cat("Trucks") };
    expect(withEntry(map, "A", cat("Cars"))).toEqual({ A: cat("Cars"), B: cat("Trucks") });
    expect(withEntry(map, "A", undefined)).toEqual({ B: cat("Trucks") });
    expect(map.A).toEqual(cat("Tools"));
  });
});

describe("setOverride", () => {
  it("writes one mod's category picks in the order they were made", async () => {
    const done: string[] = [];
    let releaseFirst!: () => void;
    vi.mocked(rawInvoke)
      .mockImplementationOnce(
        (_cmd, args) =>
          new Promise((r) => {
            releaseFirst = () => {
              done.push((args as { row: Cat }).row.category);
              r(undefined);
            };
          }),
      )
      .mockImplementationOnce((_cmd, args) => {
        done.push((args as { row: Cat }).row.category);
        return Promise.resolve(undefined);
      });
    const first = setOverride({ techName: "FS25_A", category: "Tools", subcategory: null });
    const second = setOverride({ techName: "FS25_A", category: "Trucks", subcategory: null });
    await new Promise((r) => setTimeout(r, 10));
    expect(rawInvoke).toHaveBeenCalledTimes(1); // the second waits for the first
    releaseFirst();
    await Promise.all([first, second]);
    expect(done).toEqual(["Tools", "Trucks"]);
  });
});
