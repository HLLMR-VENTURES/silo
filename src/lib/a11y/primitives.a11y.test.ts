// @vitest-environment jsdom
// axe over every src/lib/ui primitive (#258): each renders with typical props and must have no
// WCAG A/AA or best-practice violations. Overlays (menu, popover, dialog, drawer, confirm, prompt,
// toast) are checked open, since that's when their roles and names matter.
import { beforeAll, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import PrimitiveHarness from "./__tests__/PrimitiveHarness.svelte";
import DialogHarness from "../ui/__tests__/DialogHarness.svelte";
import ToastRegion from "../ui/ToastRegion.svelte";
import { toast } from "../ui/toasts.svelte";
import { axeViolations, expectNoAxeViolations } from "./axe";

beforeAll(() => {
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
});

describe("axe harness", () => {
  it("catches a nameless button and a missing alt (so a pass means something)", async () => {
    document.body.innerHTML = '<main><button type="button"></button><img src="x.png"></main>';
    const ids = (await axeViolations(document.body)).map((v) => v.id).sort();
    expect(ids).toEqual(["button-name", "image-alt"]);
    document.body.innerHTML = "";
  });
});

/** What each overlay must have put on the page before axe looks. */
const OVERLAY_ROLE: Record<string, string> = {
  menu: "menu",
  dialog: "dialog",
  drawer: "heading",
};

const INLINE = [
  "badge",
  "button",
  "iconButton",
  "switch",
  "checkbox",
  "chip",
  "tag",
  "kbd",
  "icon",
  "field",
  "radio",
  "segmented",
  "tabs",
  "progress",
  "skeleton",
  "empty",
  "error",
  "mark",
  "tooltip",
  "menuButton",
];

const OVERLAYS = ["menu", "popover", "dialog", "drawer"];

describe("axe: inline primitives", () => {
  for (const which of INLINE) {
    it(which, async () => {
      render(PrimitiveHarness, { which });
      await tick();
      await expectNoAxeViolations(document.body, { fragment: true });
    });
  }
});

describe("axe: overlays, open", () => {
  for (const which of OVERLAYS) {
    it(which, async () => {
      render(PrimitiveHarness, { which });
      await tick();
      await tick();
      if (which === "popover") expect(document.getElementById("pop")).not.toBeNull();
      else expect(screen.getAllByRole(OVERLAY_ROLE[which]).length).toBeGreaterThan(0);
      await expectNoAxeViolations(document.body, { fragment: true });
    });
  }

  it("menu button with its menu open", async () => {
    const user = userEvent.setup();
    render(PrimitiveHarness, { which: "menuButton" });
    await user.click(screen.getByRole("button", { name: "Sort" }));
    await tick();
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await expectNoAxeViolations(document.body, { fragment: true });
  });

  it("confirm dialog (danger, with facts and a checkbox)", async () => {
    const user = userEvent.setup();
    render(DialogHarness, {
      mode: "askWithOption",
      opts: {
        title: "Delete Harvest 2026?",
        body: "The set is removed. Your mods stay where they are.",
        facts: [{ label: "Mods", value: "42" }],
        confirm: "Delete set",
        tone: "danger",
        checkbox: { label: "Also delete the auto snapshots" },
      },
      onresult: () => {},
    });
    await user.click(screen.getByRole("button", { name: "Open dialog" }));
    await screen.findByRole("alertdialog");
    await expectNoAxeViolations(document.body, { fragment: true });
  });

  it("prompt dialog", async () => {
    const user = userEvent.setup();
    render(DialogHarness, {
      mode: "askText",
      opts: { title: "Save as a set", label: "Set name", confirm: "Save set", value: "" },
      onresult: () => {},
    });
    await user.click(screen.getByRole("button", { name: "Open dialog" }));
    await screen.findByRole("dialog");
    await expectNoAxeViolations(document.body, { fragment: true });
  });

  it("toast region with an action toast and an error", async () => {
    render(ToastRegion);
    toast({ message: "Switched to Harvest.", tone: "success", action: { label: "Undo", run: () => {} } });
    toast({ message: "Couldn't move the file.", tone: "danger", sticky: true, detail: "EPERM" });
    await tick();
    expect(screen.getByRole("button", { name: /^Undo/ })).toBeInTheDocument();
    await expectNoAxeViolations(document.body, { fragment: true });
  });
});
