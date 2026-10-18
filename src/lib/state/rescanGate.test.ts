import { afterEach, describe, expect, it, vi } from "vitest";
import { clearActivity } from "./activity.svelte";
import { resetIdleWaiters, running, start } from "./ops.svelte";
import { rescanGate } from "./rescanGate";

afterEach(() => {
  running.length = 0;
  resetIdleWaiters();
  clearActivity();
});

describe("rescanGate", () => {
  it("a background op (the archive move) never holds a scan back", () => {
    const run = vi.fn();
    const gate = rescanGate(run);
    start("Moving your library", { exclusive: "library-write", background: true });
    expect(gate.admit(true)).toBe(true);
    expect(gate.pending).toBe(false);
  });

  it("behind a foreground write, queues exactly one rescan that runs when the write ends", async () => {
    const run = vi.fn();
    const gate = rescanGate(run);
    const write = start("Filing 3 mods", { exclusive: "library-write" });
    expect(gate.admit(false)).toBe(false);
    expect(gate.admit(true)).toBe(false); // collapses into the same one; any auto wins
    expect(gate.admit(false)).toBe(false);
    expect(gate.pending).toBe(true);
    await Promise.resolve();
    expect(run).not.toHaveBeenCalled();
    write.end();
    await Promise.resolve();
    expect(run).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith(true);
    expect(gate.pending).toBe(false);
  });

  it("waits for the last foreground write, not the first", async () => {
    const run = vi.fn();
    const gate = rescanGate(run);
    const a = start("A", { exclusive: "library-write" });
    const b = start("B", { exclusive: "library-write" });
    gate.admit(false);
    a.end();
    await Promise.resolve();
    expect(run).not.toHaveBeenCalled();
    b.end();
    await Promise.resolve();
    expect(run).toHaveBeenCalledWith(false);
  });

  it("a caller holding the write class scans at once", () => {
    const gate = rescanGate(vi.fn());
    start("Clearing the scan cache", { exclusive: "library-write" });
    expect(gate.admit(false, true)).toBe(true);
    expect(gate.pending).toBe(false);
  });
});
