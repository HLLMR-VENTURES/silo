// @vitest-environment jsdom
// The Library's "Mod sets…" dialog (#323): tri-state boxes per saved set, the click cycle, Untick
// all, the note beside the set you're on, and Save handing over only what changed.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import type { Loadout } from "../../types";
import type { MembershipEdit } from "../../modsets/membership";
import SetMembershipDialog from "./SetMembershipDialog.svelte";

const set = (id: number, name: string, mods: string[], kind: Loadout["kind"] = "user"): Loadout => ({
  id,
  name,
  mods,
  kind,
  createdAt: null,
});
const sets = [
  set(1, "Riverbend", ["A", "B", "X"]),
  set(2, "Hutan", ["A"]),
  set(3, "Zielonka", ["X"]),
  set(4, "Before Hutan", ["A", "B"], "auto"),
];
const chosen = [
  { techName: "A", title: "Courseplay" },
  { techName: "B", title: "AutoDrive" },
];

function setup(props: Partial<{ currentId: number | null; active: Set<string>; onsave: (e: MembershipEdit[]) => Promise<boolean> }> = {}) {
  const onsave = vi.fn(props.onsave ?? (async () => true));
  render(SetMembershipDialog, {
    props: { open: true, mods: chosen, sets, currentId: props.currentId ?? null, active: props.active ?? new Set(), onsave },
  });
  const box = (name: string) => screen.getByRole("checkbox", { name: new RegExp(name) }) as HTMLInputElement;
  return { onsave, box, user: userEvent.setup() };
}

const look = (b: HTMLInputElement) => (b.indeterminate ? "mixed" : b.checked ? "ticked" : "unticked");

describe("SetMembershipDialog", () => {
  it("lists the sets you saved with ticked, mixed and unticked boxes; no automatic snapshots", () => {
    const { box } = setup();
    expect(screen.getByRole("dialog", { name: "Mod sets for 2 mods" })).toBeInTheDocument();
    expect(look(box("Riverbend"))).toBe("ticked");
    expect(look(box("Hutan"))).toBe("mixed");
    expect(look(box("Zielonka"))).toBe("unticked");
    expect(screen.queryByText("Before Hutan")).not.toBeInTheDocument();
  });

  it("a click cycles mixed → ticked → unticked → mixed", async () => {
    const { box, user } = setup();
    const seen = [look(box("Hutan"))];
    for (let i = 0; i < 3; i++) {
      await user.click(box("Hutan"));
      seen.push(look(box("Hutan")));
    }
    expect(seen).toEqual(["mixed", "ticked", "unticked", "mixed"]);
  });

  it("Save is off until something changes, then hands over only the changed sets", async () => {
    const { box, user, onsave } = setup();
    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();
    await user.click(box("Zielonka"));
    await user.click(box("Riverbend"));
    await user.click(save);
    expect(onsave).toHaveBeenCalledTimes(1);
    const edits = onsave.mock.calls[0][0];
    expect(edits.map((e) => [e.set.name, e.mods])).toEqual([
      ["Riverbend", ["X"]],
      ["Zielonka", ["X", "A", "B"]],
    ]);
    await tick();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("Untick all unticks every set; Space toggles a focused box", async () => {
    const { box, user, onsave } = setup();
    await user.click(screen.getByRole("button", { name: "Untick all" }));
    expect(["Riverbend", "Hutan", "Zielonka"].map((n) => look(box(n)))).toEqual(["unticked", "unticked", "unticked"]);
    expect(screen.getByRole("button", { name: "Untick all" })).toBeDisabled();
    box("Zielonka").focus();
    await user.keyboard(" ");
    expect(look(box("Zielonka"))).toBe("ticked");
    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onsave.mock.calls[0][0].map((e) => e.set.name)).toEqual(["Riverbend", "Hutan", "Zielonka"]);
  });

  it("says beside the set you're on what saving also does to your active mods", async () => {
    const { box, user } = setup({ currentId: 3, active: new Set(["X"]) });
    expect(screen.getByText("Active")).toBeInTheDocument();
    await user.click(box("Zielonka"));
    expect(box("Zielonka")).toHaveAccessibleDescription("also turns them on now");
    await user.click(box("Riverbend"));
    expect(screen.queryByText("also parks them now")).not.toBeInTheDocument();
  });

  it("stays open when nothing was saved", async () => {
    const { box, user } = setup({ onsave: async () => false });
    await user.click(box("Zielonka"));
    await user.click(screen.getByRole("button", { name: "Save" }));
    await tick();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("says what to do when there are no sets yet", () => {
    render(SetMembershipDialog, {
      props: { open: true, mods: [chosen[0]], sets: [sets[3]], currentId: null, active: new Set<string>(), onsave: vi.fn() },
    });
    expect(screen.getByRole("dialog", { name: "Mod sets for Courseplay" })).toBeInTheDocument();
    expect(screen.getByText("No mod sets yet")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Untick all" })).not.toBeInTheDocument();
  });
});
