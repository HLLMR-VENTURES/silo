// Optimistic edits put back after a failed save, but never over a newer edit (#358).
import { describe, expect, it, vi } from "vitest";
import { revertEntry } from "./optimistic";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(),
}));
import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { setOverride } from "../api";

type Cat = { category: string; subcategory: string | null };

describe("revertEntry", () => {
  const tools: Cat = { category: "Tools", subcategory: null };
  const trucks: Cat = { category: "Trucks", subcategory: null };

  it("puts the old value back when the entry is still the failed edit's", () => {
    const map = { A: trucks, B: tools };
    expect(revertEntry(map, "A", trucks, tools)).toEqual({ A: tools, B: tools });
    expect(map.A).toBe(trucks); // a new map; the old one is untouched
  });

  it("removes the entry when there was none before the edit", () => {
    expect(revertEntry({ A: trucks }, "A", trucks, undefined)).toEqual({});
  });

  it("leaves a newer edit alone, even one with the same values", () => {
    const newer: Cat = { category: "Trucks", subcategory: null };
    expect(revertEntry({ A: newer }, "A", trucks, tools)).toBeNull();
  });

  it("a failed removal (Use Silo's category) brings the override back", () => {
    expect(revertEntry({}, "A", undefined, tools)).toEqual({ A: tools });
  });

  it("a failed removal leaves a category picked since alone", () => {
    expect(revertEntry({ A: trucks }, "A", undefined, tools)).toBeNull();
  });

  it("doesn't read inherited keys as entries", () => {
    expect(revertEntry({}, "toString", undefined, tools)).toEqual({ toString: tools });
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
