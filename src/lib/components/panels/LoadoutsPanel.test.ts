// @vitest-environment jsdom
// LoadoutsPanel under a foreground library change: Save current, Overwrite, Delete and Import are
// off and say why; Apply stays (App refuses a switch itself). Without a lock they all work.
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import LoadoutsPanel from "./LoadoutsPanel.svelte";

const loadouts = [{ id: 1, name: "Harvest", mods: ["FS25_A", "FS25_B"] }];

function setup(lockReason = "") {
  const props = {
    loadouts,
    activeCount: 2,
    activeLoadoutId: null,
    lockReason,
    onApply: vi.fn(),
    onOverwrite: vi.fn(),
    onExport: vi.fn(),
    onRemove: vi.fn(),
    onSaveCurrent: vi.fn(),
    onImport: vi.fn(),
  };
  render(LoadoutsPanel, props);
  return props;
}

/** The in-app tooltip an icon-only button shows on hover (they carry no native title). */
async function tipText(b: HTMLElement): Promise<string> {
  b.dispatchEvent(new Event("pointerenter"));
  const tip = await waitFor(() => {
    const el = document.getElementById(b.getAttribute("aria-describedby") ?? "");
    if (!el) throw new Error("no tooltip yet");
    return el;
  });
  return tip.textContent ?? "";
}

const writes = () => [
  screen.getByRole("button", { name: /Save current active set/ }),
  screen.getByRole("button", { name: "Overwrite with current active set" }),
  screen.getByRole("button", { name: "Delete loadout" }),
  screen.getByRole("button", { name: /Import a \.silo file/ }),
];

describe("LoadoutsPanel lock", () => {
  it("turns off the buttons that change loadouts and names the reason", async () => {
    const p = setup("Silo is reclaiming disk space.");
    for (const b of writes()) {
      expect(b).toBeDisabled();
      if (b.classList.contains("lp-icon")) {
        expect(b).not.toHaveAttribute("title");
        expect(await tipText(b)).toBe("Silo is reclaiming disk space.");
      } else {
        expect(b).toHaveAttribute("title", "Silo is reclaiming disk space.");
      }
    }
    const user = userEvent.setup();
    for (const b of writes()) await user.click(b);
    expect(p.onSaveCurrent).not.toHaveBeenCalled();
    expect(p.onOverwrite).not.toHaveBeenCalled();
    expect(p.onRemove).not.toHaveBeenCalled();
    expect(p.onImport).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: /Export to a \.silo file/ })).toBeEnabled();
  });

  it("works normally with no lock", async () => {
    const p = setup();
    for (const b of writes()) expect(b).toBeEnabled();
    await userEvent.setup().click(writes()[2]);
    expect(p.onRemove).toHaveBeenCalledWith(loadouts[0]);
  });
});
