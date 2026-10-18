// @vitest-environment jsdom
// Behavior and a11y of the interactive primitives: Switch, Segmented, Tabs, Field.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Switch from "./Switch.svelte";
import Segmented from "./Segmented.svelte";
import FieldHarness from "./__tests__/FieldHarness.svelte";
import TabsHarness from "./__tests__/TabsHarness.svelte";

describe("Switch", () => {
  it("is a switch named by its label, and flips aria-checked on click", async () => {
    const onchange = vi.fn();
    render(Switch, { label: "Show hidden mods", onchange });
    const sw = screen.getByRole("switch", { name: "Show hidden mods" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    await userEvent.click(sw);
    expect(sw).toHaveAttribute("aria-checked", "true");
    expect(onchange).toHaveBeenLastCalledWith(true);
  });

  it("toggles on Space and Enter", async () => {
    const user = userEvent.setup();
    const onchange = vi.fn();
    render(Switch, { ariaLabel: "Auto-update", checked: true, onchange });
    const sw = screen.getByRole("switch", { name: "Auto-update" });
    sw.focus();
    await user.keyboard(" ");
    expect(sw).toHaveAttribute("aria-checked", "false");
    expect(onchange).toHaveBeenLastCalledWith(false);
    await user.keyboard("{Enter}");
    expect(sw).toHaveAttribute("aria-checked", "true");
    expect(onchange).toHaveBeenCalledTimes(2);
  });

  it("does nothing when disabled", async () => {
    const onchange = vi.fn();
    render(Switch, { label: "Locked", disabled: true, onchange });
    await userEvent.click(screen.getByRole("switch", { name: "Locked" }));
    expect(onchange).not.toHaveBeenCalled();
  });
});

describe("Segmented", () => {
  const options = [
    { value: "grid", label: "Grid" },
    { value: "list", label: "List" },
    { value: "table", label: "Table" },
  ];

  it("exposes a radiogroup with one checked radio in the tab order", () => {
    render(Segmented, { options, value: "list", ariaLabel: "View" });
    expect(screen.getByRole("radiogroup", { name: "View" })).toBeInTheDocument();
    const radios = screen.getAllByRole("radio");
    expect(radios.map((r) => r.getAttribute("aria-checked"))).toEqual(["false", "true", "false"]);
    expect(radios.map((r) => r.tabIndex)).toEqual([-1, 0, -1]);
  });

  it("Arrow Right moves selection and focus, wrapping at the end", async () => {
    const user = userEvent.setup();
    const onchange = vi.fn();
    render(Segmented, { options, value: "list", ariaLabel: "View", onchange });
    screen.getByRole("radio", { name: "List" }).focus();
    await user.keyboard("{ArrowRight}");
    const table = screen.getByRole("radio", { name: "Table" });
    expect(table).toHaveAttribute("aria-checked", "true");
    expect(table).toHaveFocus();
    expect(table.tabIndex).toBe(0);
    expect(onchange).toHaveBeenLastCalledWith("table");
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("radio", { name: "Grid" })).toHaveFocus();
  });

  it("Home and End jump to the first and last segment", async () => {
    const user = userEvent.setup();
    render(Segmented, { options, value: "list", ariaLabel: "View" });
    screen.getByRole("radio", { name: "List" }).focus();
    await user.keyboard("{End}");
    expect(screen.getByRole("radio", { name: "Table" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Table" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("radio", { name: "Grid" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "Grid" })).toHaveFocus();
  });

  it("selects on click", async () => {
    render(Segmented, { options, value: "grid", ariaLabel: "View" });
    await userEvent.click(screen.getByRole("radio", { name: "Table" }));
    expect(screen.getByRole("radio", { name: "Table" })).toHaveAttribute("aria-checked", "true");
  });
});

describe("Tabs", () => {
  const tabs = [
    { id: "overview", label: "Overview" },
    { id: "files", label: "Files", badge: 3 },
    { id: "history", label: "History" },
  ];

  it("wires tablist, tabs and the panel together", () => {
    render(TabsHarness, { tabs, selected: "files" });
    expect(screen.getByRole("tablist", { name: "Details" })).toBeInTheDocument();
    const files = screen.getByRole("tab", { name: /Files/ });
    expect(files).toHaveAttribute("aria-selected", "true");
    expect(files.tabIndex).toBe(0);
    expect(screen.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "false");
    const panel = screen.getByRole("tabpanel");
    expect(files).toHaveAttribute("aria-controls", panel.id);
    expect(panel).toHaveAttribute("aria-labelledby", files.id);
    expect(panel).toHaveTextContent("Panel for files");
  });

  it("defaults to the first tab", () => {
    render(TabsHarness, { tabs });
    expect(screen.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Panel for overview");
  });

  it("arrow keys, Home and End move focus and activate the tab", async () => {
    const user = userEvent.setup();
    render(TabsHarness, { tabs, selected: "overview" });
    screen.getByRole("tab", { name: "Overview" }).focus();

    await user.keyboard("{ArrowRight}");
    const files = screen.getByRole("tab", { name: /Files/ });
    expect(files).toHaveFocus();
    expect(files).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Panel for files");

    await user.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "History" })).toHaveFocus();
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Panel for history");

    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "Overview" })).toHaveAttribute("aria-selected", "true");
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "History" })).toHaveAttribute("aria-selected", "true");
  });

  it("selects on click", async () => {
    render(TabsHarness, { tabs });
    await userEvent.click(screen.getByRole("tab", { name: "History" }));
    expect(screen.getByRole("tabpanel")).toHaveTextContent("Panel for history");
  });
});

describe("Field", () => {
  const describedTexts = (el: HTMLElement) =>
    (el.getAttribute("aria-describedby") ?? "")
      .split(/\s+/)
      .filter(Boolean)
      .map((id) => document.getElementById(id)?.textContent?.trim());

  it("labels the control by id", () => {
    render(FieldHarness, {});
    const input = screen.getByLabelText("Mods folder");
    expect(input.tagName).toBe("INPUT");
    expect(input.id).not.toBe("");
    expect(input).not.toHaveAttribute("aria-describedby");
    expect(input).not.toHaveAttribute("aria-invalid");
  });

  it("describes the control by its hint", () => {
    render(FieldHarness, { hint: "Where FS25 loads mods from" });
    const input = screen.getByLabelText("Mods folder");
    expect(describedTexts(input)).toEqual(["Where FS25 loads mods from"]);
    expect(input).toHaveAccessibleDescription("Where FS25 loads mods from");
  });

  it("adds the error to the description and marks the control invalid", () => {
    render(FieldHarness, { hint: "Where FS25 loads mods from", error: "Folder not found" });
    const input = screen.getByLabelText("Mods folder");
    expect(describedTexts(input)).toEqual(["Where FS25 loads mods from", "Folder not found"]);
    expect(input).toHaveAttribute("aria-invalid", "true");
    const err = document.getElementById(input.getAttribute("aria-describedby")!.split(" ")[1])!;
    expect(err).toHaveAttribute("aria-live", "polite");
  });

  it("updates when the error clears", async () => {
    const { rerender } = render(FieldHarness, { error: "Folder not found" });
    const input = screen.getByLabelText("Mods folder");
    expect(input).toHaveAttribute("aria-invalid", "true");
    await rerender({ error: undefined });
    expect(input).not.toHaveAttribute("aria-invalid");
    expect(input).not.toHaveAttribute("aria-describedby");
  });
});
