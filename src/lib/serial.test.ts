import { describe, expect, it } from "vitest";
import { serial, serialDrained, serialPending, serialPendingUnder } from "./serial";

/** A task that finishes after `ms` and records when it started and ended. */
function task(log: string[], name: string, ms: number, fail = false) {
  return () =>
    new Promise<string>((resolve, reject) => {
      log.push(`start ${name}`);
      setTimeout(() => {
        log.push(`end ${name}`);
        if (fail) reject(new Error(name));
        else resolve(name);
      }, ms);
    });
}

describe("serial", () => {
  it("runs calls with the same key one at a time, in call order", async () => {
    const log: string[] = [];
    // The first write is the slowest: without the queue it would land last.
    const a = serial("curation:FS25_A", task(log, "1", 30));
    const b = serial("curation:FS25_A", task(log, "2", 5));
    const c = serial("curation:FS25_A", task(log, "3", 1));
    expect(await Promise.all([a, b, c])).toEqual(["1", "2", "3"]);
    expect(log).toEqual(["start 1", "end 1", "start 2", "end 2", "start 3", "end 3"]);
  });

  it("lets different keys run side by side", async () => {
    const log: string[] = [];
    const a = serial("tags:FS25_A", task(log, "a", 20));
    const b = serial("tags:FS25_B", task(log, "b", 1));
    await Promise.all([a, b]);
    expect(log).toEqual(["start a", "start b", "end b", "end a"]);
  });

  it("keeps going after a failed call and still reports the failure to its caller", async () => {
    const log: string[] = [];
    const a = serial("k-fail", task(log, "bad", 5, true));
    const b = serial("k-fail", task(log, "good", 1));
    await expect(a).rejects.toThrow("bad");
    expect(await b).toBe("good");
    expect(log).toEqual(["start bad", "end bad", "start good", "end good"]);
  });

  it("forgets a key once its last call settles", async () => {
    const a = serial("k-clean", () => Promise.resolve(1));
    const b = serial("k-clean", () => Promise.reject(new Error("x")));
    expect(serialPending("k-clean")).toBe(true);
    await a;
    await b.catch(() => {});
    expect(serialPending("k-clean")).toBe(false);
  });

  it("serialDrained waits for every call under a prefix, failed ones too, and no others", async () => {
    const log: string[] = [];
    void serial("override:A", task(log, "a1", 10, true)).catch(() => {});
    void serial("override:A", task(log, "a2", 5)).catch(() => {});
    void serial("override:B", task(log, "b", 15)).catch(() => {});
    let release!: () => void;
    const other = serial("curation:A", () => new Promise<void>((r) => (release = r)));
    expect(serialPendingUnder("override:")).toBe(true);
    await serialDrained("override:"); // settles while the curation write is still held
    expect(log).toEqual(expect.arrayContaining(["end a1", "end a2", "end b"]));
    expect(serialPendingUnder("override:")).toBe(false);
    expect(serialPending("curation:A")).toBe(true);
    release();
    await other;
  });

  it("serialDrained also waits for a call queued while it waits", async () => {
    const log: string[] = [];
    void serial("drain:A", task(log, "first", 10));
    const drained = serialDrained("drain:");
    setTimeout(() => void serial("drain:B", task(log, "later", 20)).catch(() => {}), 5);
    await drained;
    expect(log).toContain("end later");
  });

  it("serialDrained with nothing queued settles at once", async () => {
    await expect(serialDrained("nothing-here:")).resolves.toBeUndefined();
  });
});
