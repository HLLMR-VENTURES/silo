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
    const o = parkLooseOptions("Courseplay", "Scripts & Tools");
    expect(o.title).toBe("Park Courseplay?");
    expect(o.body).toContain("archive\\Scripts & Tools");
    expect(o.confirm).toBe("Park it");
    expect(o.checkbox).toEqual({ label: "Don't explain this again" });
    expect(parkLooseOptions("X", "Maps", "/").body).toContain("archive/Maps");
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
