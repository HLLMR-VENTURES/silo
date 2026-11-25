import { describe, expect, it } from "vitest";
import { gameUpdater } from "./gameState";
import type { GameState } from "./api";
import type { GameInfo } from "./types";

const install = (installDir: string, chosen = false): GameInfo => ({
  appId: "2300320",
  exe: `${installDir}/FarmingSimulator2025.exe`,
  installDir,
  store: chosen ? "standalone" : "steam",
  chosen,
});
const STEAM: GameState = { install: install("E:/SteamLibrary/steamapps/common/Farming Simulator 25"), choice: null };
const DVD: GameState = {
  install: install("E:/Farming Simulator 2025", true),
  choice: "E:/Farming Simulator 2025/FarmingSimulator2025.exe",
};

function harness() {
  let shown: GameState = { install: null, choice: null };
  const applied: { state: GameState; changed: boolean }[] = [];
  const update = gameUpdater(
    (state, changed) => {
      shown = state;
      applied.push({ state, changed });
    },
    () => shown.install?.installDir ?? null,
  );
  return { update, applied, shown: () => shown };
}

/** A promise the test settles when it wants. */
function later<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => ((resolve = res), (reject = rej)));
  return { promise, resolve, reject };
}

describe("gameUpdater (#461)", () => {
  it("runs updates one after another, so a slow detection can't undo a choice", async () => {
    const h = harness();
    const slowDetect = later<GameState>();
    const detecting = h.update(() => slowDetect.promise);
    let choiceAsked = false;
    // The choice starts only once the detection is done.
    const choosing = h.update(async () => {
      choiceAsked = true;
      return DVD;
    });
    await Promise.resolve();
    expect(choiceAsked).toBe(false);
    slowDetect.resolve(STEAM);
    await Promise.all([detecting, choosing]);
    expect(h.shown()).toEqual(DVD);
    expect(h.applied.map((a) => a.state)).toEqual([STEAM, DVD]);
  });

  it("shows the install and the choice from the same answer", async () => {
    const h = harness();
    await h.update(async () => DVD);
    expect(h.shown().install?.installDir).toBe("E:/Farming Simulator 2025");
    expect(h.shown().choice).toBe(DVD.choice);
  });

  it("says when the install folder changed, so conflicts are re-checked", async () => {
    const h = harness();
    await h.update(async () => STEAM);
    await h.update(async () => STEAM);
    await h.update(async () => DVD);
    await h.update(async () => ({ install: null, choice: DVD.choice }));
    expect(h.applied.map((a) => a.changed)).toEqual([true, false, true, true]);
  });

  it("applies nothing from a failed update, and the next one still runs", async () => {
    const h = harness();
    await h.update(async () => STEAM);
    await expect(h.update(async () => Promise.reject(new Error("disk")))).rejects.toThrow("disk");
    expect(h.shown()).toEqual(STEAM);
    await h.update(async () => DVD);
    expect(h.shown()).toEqual(DVD);
  });
});
