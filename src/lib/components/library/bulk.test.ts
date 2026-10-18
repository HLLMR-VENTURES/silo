// @vitest-environment jsdom
// Bulk Turn on / Park (#242): which selected mods change, the >25 confirmation (with counts), and
// the BulkBar that runs them.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import BulkBar from "./BulkBar.svelte";
import { BULK_CONFIRM_OVER, bulkChanges, bulkConfirm, confirmBulkChange } from "./bulk";
import type { ConfirmOptions } from "../../ui/dialogs.svelte";

describe("bulkChanges", () => {
  const mods = ["A", "B", "C"].map((techName) => ({ techName }));
  it("leaves out the mods already as asked", () => {
    const active = new Set(["A"]);
    expect(bulkChanges(mods, true, active).map((m) => m.techName)).toEqual(["B", "C"]);
    expect(bulkChanges(mods, false, active).map((m) => m.techName)).toEqual(["A"]);
  });
});

describe("the >25 confirmation", () => {
  it("doesn't ask at 25 or fewer", async () => {
    const ask = vi.fn((_o: ConfirmOptions) => Promise.resolve(true));
    expect(bulkConfirm(BULK_CONFIRM_OVER, true, false)).toBeNull();
    expect(await confirmBulkChange(25, false, false, ask)).toBe(true);
    expect(ask).not.toHaveBeenCalled();
  });

  it("asks with the counts over 25, and Cancel stops it", async () => {
    const ask = vi.fn((_o: ConfirmOptions) => Promise.resolve(false));
    expect(await confirmBulkChange(26, false, false, ask)).toBe(false);
    expect(ask).toHaveBeenCalledTimes(1);
    expect(ask.mock.calls[0][0]).toMatchObject({
      title: "Park 26 mods?",
      confirm: "Park 26 mods",
      facts: [{ label: "Park", value: "26 mods" }],
    });
    const yes = vi.fn((_o: ConfirmOptions) => Promise.resolve(true));
    expect(await confirmBulkChange(40, true, true, yes)).toBe(true);
    const opts = yes.mock.calls[0][0];
    expect(opts.title).toBe("Turn on 40 mods?");
    expect(opts.body).toContain("cloud-synced");
  });
});

describe("BulkBar", () => {
  function setup(extra: Record<string, unknown> = {}) {
    const h = {
      onturnon: vi.fn(),
      onpark: vi.fn(),
      onfavorite: vi.fn(),
      onhide: vi.fn(),
      onselectall: vi.fn(),
      onclear: vi.fn(),
    };
    render(BulkBar, { count: 12, shownCount: 140, ...h, ...extra });
    return { user: userEvent.setup(), ...h };
  }

  it("shows the count and runs each action", async () => {
    const { user, ...h } = setup();
    expect(screen.getByRole("toolbar", { name: "Selected mods" })).toHaveTextContent("12 selected");
    await user.click(screen.getByRole("button", { name: "Turn on" }));
    await user.click(screen.getByRole("button", { name: "Park" }));
    await user.click(screen.getByRole("button", { name: "Favorite" }));
    await user.click(screen.getByRole("button", { name: "Hide" }));
    await user.click(screen.getByRole("button", { name: "Select all 140 shown" }));
    await user.click(screen.getByRole("button", { name: "Clear" }));
    expect(h.onturnon).toHaveBeenCalledTimes(1);
    expect(h.onpark).toHaveBeenCalledTimes(1);
    expect(h.onfavorite).toHaveBeenCalledWith(true);
    expect(h.onhide).toHaveBeenCalledWith(true);
    expect(h.onselectall).toHaveBeenCalledTimes(1);
    expect(h.onclear).toHaveBeenCalledTimes(1);
  });

  it("flips Favorite / Hide when every selected mod already is one, and turns off switching during a write", async () => {
    const { user, ...h } = setup({ allFavorite: true, allHidden: true, writeReason: "Silo is moving mods.", allShownSelected: true });
    await user.click(screen.getByRole("button", { name: "Remove favorite" }));
    await user.click(screen.getByRole("button", { name: "Unhide" }));
    expect(h.onfavorite).toHaveBeenCalledWith(false);
    expect(h.onhide).toHaveBeenCalledWith(false);
    expect(screen.getByRole("button", { name: "Turn on" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Park" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Select all 140 shown" })).toBeDisabled();
  });
});
