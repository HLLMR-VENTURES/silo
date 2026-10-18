// Parking a loose mod (#242): the explanation shows the first time, then not again; "Don't
// explain this again" keeps it off for good; Cancel stops the park and asks again next time.
import { describe, expect, it, vi } from "vitest";
import { createParkLooseExplainer, PARK_LOOSE_KEY, parkLooseOptions } from "./parkLoose";
import type { ConfirmOptions } from "../../ui/dialogs.svelte";

function memStorage() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), m };
}

describe("loose-mod park explanation", () => {
  it("names the mod and its archive folder, with the option", () => {
    const o = parkLooseOptions(1, "Courseplay", "Scripts & Tools");
    expect(o.title).toBe("Park Courseplay?");
    expect(o.body).toContain("archive\\Scripts & Tools");
    expect(o.confirm).toBe("Park it");
    expect(o.checkbox).toEqual({ label: "Don't explain this again" });
    expect(parkLooseOptions(1, "X", "Maps", "/").body).toContain("archive/Maps");
    expect(o.body).toBe(
      "Silo moves it into your library folder (archive\\Scripts & Tools) so the game stops loading it. " +
        "It stays in your library. Switch it on any time.",
    );
  });

  it("several: plural copy, no placeholder folder", () => {
    const o = parkLooseOptions(3, "3 mods", null);
    expect(o.title).toBe("Park 3 mods?");
    expect(o.body).toBe(
      "Silo moves them into your library folder, each into its category folder in archive, so the game stops loading them. " +
        "They stay in your library. Switch them on any time.",
    );
    expect(o.body).not.toContain("<");
    expect(o.confirm).toBe("Park them");
    expect(o.checkbox).toEqual({ label: "Don't explain this again" });
  });

  it("the explainer passes the count through", async () => {
    const ask = vi.fn((_o: ConfirmOptions) => Promise.resolve({ confirmed: true, checked: false }));
    await createParkLooseExplainer(ask, memStorage())("2 mods", null, 2);
    expect(ask.mock.calls[0][0].title).toBe("Park 2 mods?");
    expect(ask.mock.calls[0][0].body).toContain("each into its category folder in archive");
  });

  it("shows once, then not; ticked, it's remembered", async () => {
    const st = memStorage();
    const ask = vi.fn((_o: ConfirmOptions) => Promise.resolve({ confirmed: true, checked: true }));
    const explain = createParkLooseExplainer(ask, st);
    expect(await explain("Courseplay", "Scripts & Tools")).toBe(true);
    expect(await explain("Other", "Tractors")).toBe(true);
    expect(ask).toHaveBeenCalledTimes(1);
    expect(st.m.get(PARK_LOOSE_KEY)).toBe("1");
    // A later session reads the flag and doesn't ask.
    const later = vi.fn(() => Promise.resolve({ confirmed: true, checked: false }));
    expect(await createParkLooseExplainer(later, st)("X", "Maps")).toBe(true);
    expect(later).not.toHaveBeenCalled();
  });

  it("unticked: not again this session, but asked next session", async () => {
    const st = memStorage();
    const ask = vi.fn(() => Promise.resolve({ confirmed: true, checked: false }));
    const explain = createParkLooseExplainer(ask, st);
    await explain("A", "Maps");
    await explain("B", "Maps");
    expect(ask).toHaveBeenCalledTimes(1);
    expect(st.m.has(PARK_LOOSE_KEY)).toBe(false);
    await createParkLooseExplainer(ask, st)("C", "Maps");
    expect(ask).toHaveBeenCalledTimes(2);
  });

  it("Cancel stops the park and asks again next time", async () => {
    const ask = vi
      .fn()
      .mockResolvedValueOnce({ confirmed: false, checked: true })
      .mockResolvedValueOnce({ confirmed: true, checked: false });
    const st = memStorage();
    const explain = createParkLooseExplainer(ask, st);
    expect(await explain("A", "Maps")).toBe(false);
    expect(st.m.has(PARK_LOOSE_KEY)).toBe(false);
    expect(await explain("A", "Maps")).toBe(true);
    expect(ask).toHaveBeenCalledTimes(2);
  });
});
