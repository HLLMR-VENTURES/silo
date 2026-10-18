// @vitest-environment jsdom
// Behavior and a11y of the display primitives: ProgressBar, Chip.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import ProgressBar from "./ProgressBar.svelte";
import Chip from "./Chip.svelte";

describe("ProgressBar", () => {
  it("reports value, min and max when determinate", () => {
    render(ProgressBar, { value: 214, max: 612, label: "214 of 612 mods" });
    const bar = screen.getByRole("progressbar", { name: "214 of 612 mods" });
    expect(bar).toHaveAttribute("aria-valuenow", "214");
    expect(bar).toHaveAttribute("aria-valuemin", "0");
    expect(bar).toHaveAttribute("aria-valuemax", "612");
    expect(screen.getByText("214 of 612 mods")).toBeVisible();
  });

  it("clamps the value into range", () => {
    render(ProgressBar, { value: 150, label: "Done" });
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
  });

  it("omits aria-valuenow when indeterminate", () => {
    render(ProgressBar, { label: "Scanning mods" });
    const bar = screen.getByRole("progressbar", { name: "Scanning mods" });
    expect(bar).not.toHaveAttribute("aria-valuenow");
    expect(bar).toHaveClass("indeterminate");
  });
});

describe("Chip", () => {
  it("renders a labelled remove button that calls onremove", async () => {
    const onremove = vi.fn();
    render(Chip, { label: "Category: Tractors", onremove });
    const btn = screen.getByRole("button", { name: "Remove filter: Category: Tractors" });
    await userEvent.click(btn);
    expect(onremove).toHaveBeenCalledOnce();
  });

  it("has no remove button without onremove", () => {
    render(Chip, { label: "Map: Riverbend" });
    expect(screen.getByText("Map: Riverbend")).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });
});
