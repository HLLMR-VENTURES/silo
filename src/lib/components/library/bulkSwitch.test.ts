// The bulk Turn on / Park sequence (#242): the filing dialog every time loose mods are parked in
// bulk (one dialog with the >25 question), loose mods filed first, refusal stops before anything
// changes, partial filing still offers Undo with a warning, and the plain projected paths.
import { describe, expect, it, vi } from "vitest";
import {
  bulkParkLooseConfirm,
  bulkResultMessage,
  driveOf,
  LOOSE_LIST_MAX,
  onOtherDrive,
  runBulkSwitch,
  type BulkMod,
  type BulkSwitchDeps,
  type ProjectedResult,
} from "./bulkSwitch";
import type { ConfirmOptions } from "../../ui/dialogs.svelte";
import type { ToastOptions } from "../../ui/toasts.svelte";
import type { SwitchUndoOffer } from "../../state/switchUndo";

type Mod = BulkMod & { loose?: boolean };
const mod = (techName: string, loose = false, category = "Tractors"): Mod => ({
  techName,
  title: `T-${techName}`,
  category,
  loose,
});

function setup(
  active: string[],
  over: Partial<BulkSwitchDeps<Mod>> & { fileResult?: boolean | null; projResult?: ProjectedResult } = {},
) {
  let set = new Set(active);
  const calls: string[] = [];
  const toasts: ToastOptions[] = [];
  const undos: SwitchUndoOffer[] = [];
  const ask = vi.fn(async (_o: ConfirmOptions) => true);
  const explain = vi.fn(async (_n: string, _c: string) => true);
  const fileLoose = vi.fn(async (names: string[]) => {
    calls.push(`file:${names.join(",")}`);
    const r = over.fileResult === undefined ? true : over.fileResult;
    if (r !== null) set = new Set([...set].filter((t) => !names.includes(t)));
    return r;
  });
  const applyProjected = vi.fn(async (names: string[], on: boolean) => {
    calls.push(`apply:${on ? "on" : "off"}:${names.join(",")}`);
    const r = over.projResult === undefined ? true : over.projResult;
    if (r !== null && r !== "failed") {
      set = new Set(set);
      for (const n of names) {
        if (on) set.add(n);
        else set.delete(n);
      }
    }
    return r;
  });
  const deps: BulkSwitchDeps<Mod> = {
    active: () => set,
    isLoose: (m) => !!m.loose,
    busy: () => null,
    cloudSynced: false,
    archiveOnOtherDrive: false,
    ask,
    explain,
    fileLoose,
    stillLoose: () => 0,
    applyProjected,
    toast: (o) => void toasts.push(o),
    offerUndo: (o) => void undos.push(o),
    ...over,
  };
  return { deps, ask, explain, fileLoose, applyProjected, calls, toasts, undos };
}

describe("runBulkSwitch: nothing to do, busy", () => {
  it("says when they're already as asked", async () => {
    const t = setup(["A"]);
    expect(await runBulkSwitch([mod("A"), mod("B")], false, { ...t.deps, active: () => new Set() })).toBe(
      "nothing",
    );
    expect(t.toasts[0].message).toBe("They're already parked.");
  });

  it("refuses before any dialog while another write runs", async () => {
    const t = setup(["A", "B"], { busy: () => "Silo is moving mods. Try again when it's done." });
    expect(await runBulkSwitch([mod("A", true), mod("B", true)], false, t.deps)).toBe("busy");
    expect(t.ask).not.toHaveBeenCalled();
    expect(t.fileLoose).not.toHaveBeenCalled();
    expect(t.toasts[0]).toMatchObject({ tone: "warn" });
  });
});

describe("runBulkSwitch: projected only", () => {
  it("a few mods: no dialog, applies, offers Undo", async () => {
    const t = setup(["A", "B", "C"]);
    expect(await runBulkSwitch([mod("A"), mod("B")], false, t.deps)).toBe("done");
    expect(t.ask).not.toHaveBeenCalled();
    expect(t.calls).toEqual(["apply:off:A,B"]);
    expect(t.undos[0]).toMatchObject({ message: "Parked 2 mods.", tone: "neutral", previous: ["A", "B", "C"], expected: ["C"] });
  });

  it("over 25 asks first; Cancel changes nothing", async () => {
    const mods = Array.from({ length: 26 }, (_, i) => mod(`M${i}`));
    const t = setup([]);
    t.ask.mockResolvedValueOnce(false);
    expect(await runBulkSwitch(mods, true, t.deps)).toBe("cancelled");
    expect(t.ask.mock.calls[0][0].title).toBe("Turn on 26 mods?");
    expect(t.applyProjected).not.toHaveBeenCalled();
  });

  it("refused: nothing changed, no Undo", async () => {
    const t = setup(["A", "B"], { projResult: null });
    expect(await runBulkSwitch([mod("A"), mod("B")], false, t.deps)).toBe("refused");
    expect(t.undos).toHaveLength(0);
  });

  it("threw: reported by the step, no Undo", async () => {
    const t = setup(["A", "B"], { projResult: "failed" });
    expect(await runBulkSwitch([mod("A"), mod("B")], false, t.deps)).toBe("failed");
    expect(t.undos).toHaveLength(0);
  });

  it("finished with problems: Undo with a warning", async () => {
    const t = setup([], { projResult: false });
    await runBulkSwitch([mod("A"), mod("B")], true, t.deps);
    expect(t.undos[0]).toMatchObject({ message: "Turning on 2 mods finished with problems.", tone: "warn" });
  });
});

describe("runBulkSwitch: parking loose mods", () => {
  it("always asks in bulk, even with the explanation dismissed, then files them", async () => {
    const t = setup(["A", "B"]);
    expect(await runBulkSwitch([mod("A", true), mod("B", true)], false, t.deps)).toBe("done");
    expect(t.explain).not.toHaveBeenCalled();
    expect(t.ask).toHaveBeenCalledTimes(1);
    const o = t.ask.mock.calls[0][0];
    expect(o.title).toBe("Park 2 mods?");
    expect(o.confirm).toBe("Park 2 mods");
    expect(o.body).toContain("moves them into your library folder");
    expect(o.body).toContain("Nothing is deleted; Undo puts them back.");
    expect(t.calls).toEqual(["file:A,B"]);
    expect(t.undos[0]).toMatchObject({ message: "Parked 2 mods.", tone: "neutral", previous: ["A", "B"], expected: [] });
  });

  it("Cancel files nothing", async () => {
    const t = setup(["A", "B"]);
    t.ask.mockResolvedValueOnce(false);
    expect(await runBulkSwitch([mod("A", true), mod("B")], false, t.deps)).toBe("cancelled");
    expect(t.calls).toEqual([]);
  });

  it("mixed: one dialog (with the >25 question folded in), loose filed first, then the rest", async () => {
    const mods = [...Array.from({ length: 27 }, (_, i) => mod(`P${i}`)), mod("L1", true), mod("L2", true)];
    const t = setup(mods.map((m) => m.techName));
    await runBulkSwitch(mods, false, t.deps);
    expect(t.ask).toHaveBeenCalledTimes(1);
    const o = t.ask.mock.calls[0][0];
    expect(o.title).toBe("Park 29 mods?");
    expect(o.body).toContain("2 of them aren't filed yet, so Silo moves those into your library folder (archive\\Tractors)");
    expect(o.facts).toEqual([
      { label: "Park", value: "29 mods" },
      { label: "Filed into the library", value: "2 loose" },
    ]);
    expect(t.calls[0]).toBe("file:L1,L2");
    expect(t.calls[1]).toMatch(/^apply:off:P0,/);
    expect(t.undos[0].message).toBe("Parked 29 mods.");
  });

  it("filing refused: stops before switching anything, no Undo", async () => {
    const t = setup(["A", "L"], { fileResult: null });
    expect(await runBulkSwitch([mod("A"), mod("L", true)], false, t.deps)).toBe("refused");
    expect(t.applyProjected).not.toHaveBeenCalled();
    expect(t.undos).toHaveLength(0);
  });

  it("filing partly failed: still offers Undo, warning how many couldn't be filed", async () => {
    const t = setup(["A", "L1", "L2", "L3"], { fileResult: false, stillLoose: () => 2 });
    await runBulkSwitch([mod("A"), mod("L1", true), mod("L2", true), mod("L3", true)], false, t.deps);
    expect(t.applyProjected).toHaveBeenCalled();
    expect(t.undos[0]).toMatchObject({ message: "Parked 2 mods, but 2 couldn't be filed.", tone: "warn" });
  });

  it("filed, then the second step refused: Undo for what ran, saying the rest wasn't changed", async () => {
    const t = setup(["A", "B", "L"], { projResult: null });
    expect(await runBulkSwitch([mod("A"), mod("B"), mod("L", true)], false, t.deps)).toBe("done");
    expect(t.undos[0]).toMatchObject({ message: "Parked 1 mod, but 2 weren't changed.", tone: "warn" });
  });

  it("one loose mod on its own: the one-time explanation instead", async () => {
    const t = setup(["L"]);
    await runBulkSwitch([mod("L", true, "Maps")], false, t.deps);
    expect(t.ask).not.toHaveBeenCalled();
    expect(t.explain).toHaveBeenCalledWith("T-L", "Maps");
    expect(t.calls).toEqual(["file:L"]);
  });

  it("Turn on never files anything", async () => {
    const t = setup([]);
    await runBulkSwitch([mod("A", true), mod("B", true)], true, t.deps);
    expect(t.fileLoose).not.toHaveBeenCalled();
    expect(t.calls).toEqual(["apply:on:A,B"]);
  });
});

describe("the bulk filing dialog", () => {
  it("lists up to 50 names, then how many more", () => {
    const loose = Array.from({ length: 60 }, (_, i) => mod(`L${i}`, true));
    const o = bulkParkLooseConfirm(60, loose);
    expect(o.list).toHaveLength(LOOSE_LIST_MAX + 1);
    expect(o.list?.[LOOSE_LIST_MAX]).toBe("…and 10 more");
    expect(o.body).toContain("They aren't filed yet, so Silo moves them");
  });

  it("several categories, another drive, cloud-synced over 25", () => {
    const loose = [mod("A", true, "Maps"), mod("B", true, "Tractors")];
    const o = bulkParkLooseConfirm(30, loose, { otherDrive: true, cloudSynced: true, sep: "/" });
    expect(o.body).toContain("(each into its category folder in archive)");
    expect(o.body).toContain("cloud-synced");
    expect(o.facts).toContainEqual({ label: "Library folder", value: "On another drive" });
    expect(bulkParkLooseConfirm(2, [mod("A", true, "Maps")], { sep: "/" }).body).toContain(
      "1 of them isn't filed yet, so Silo moves it into your library folder (archive/Maps)",
    );
  });

  it("result messages", () => {
    expect(bulkResultMessage(["A", "B"], false, { unfiled: 0, skipped: 0, problems: false })).toEqual({
      message: "Parked 2 mods.",
      tone: "neutral",
    });
    expect(bulkResultMessage(["A", "B", "C"], false, { unfiled: 1, skipped: 0, problems: true }).message).toBe(
      "Parked 2 mods, but 1 couldn't be filed.",
    );
  });
});

describe("drives", () => {
  it("reads drive letters and shares, case-insensitively", () => {
    expect(driveOf("d:\\Silo\\archive")).toBe("D:");
    expect(driveOf("\\\\NAS\\Games\\mods")).toBe("\\\\nas\\games");
    expect(driveOf("/home/me/mods")).toBeNull();
    expect(onOtherDrive("D:\\archive", "C:\\Users\\me\\mods")).toBe(true);
    expect(onOtherDrive("c:\\archive", "C:\\Users\\me\\mods")).toBe(false);
    expect(onOtherDrive(null, "C:\\mods")).toBe(false);
    expect(onOtherDrive("/mnt/b/archive", "/home/me/mods")).toBe(false);
  });
});
