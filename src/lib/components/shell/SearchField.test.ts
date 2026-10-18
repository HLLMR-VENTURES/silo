// @vitest-environment jsdom
// SearchField: search landmark, Esc clears then blurs, Enter submits, focus() export.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import SearchField from "./SearchField.svelte";

describe("SearchField", () => {
  it("sits in a search landmark with the placeholder and a Ctrl K hint", () => {
    const { container } = render(SearchField, { placeholder: "Search the catalog", label: "Search the catalog" });
    expect(screen.getByRole("search")).toBeInTheDocument();
    const input = screen.getByRole("searchbox", { name: "Search the catalog" });
    expect(input).toHaveAttribute("placeholder", "Search the catalog");
    expect(input).toHaveAttribute("aria-keyshortcuts", "Control+K /");
    expect(container.querySelectorAll("kbd")).toHaveLength(2);
  });

  it("Esc clears the text first, then blurs", async () => {
    const user = userEvent.setup();
    const onclear = vi.fn();
    render(SearchField, { onclear });
    const input = screen.getByRole("searchbox");
    await user.click(input);
    await user.keyboard("tractor");
    expect(input).toHaveValue("tractor");
    await user.keyboard("{Escape}");
    expect(input).toHaveValue("");
    expect(onclear).toHaveBeenCalledTimes(1);
    expect(input).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(input).not.toHaveFocus();
  });

  it("Enter submits the value", async () => {
    const user = userEvent.setup();
    const onsubmit = vi.fn();
    render(SearchField, { onsubmit });
    await user.click(screen.getByRole("searchbox"));
    await user.keyboard("plough{Enter}");
    expect(onsubmit).toHaveBeenCalledWith("plough");
  });

  it("focus() focuses and selects", async () => {
    const { component } = render(SearchField, { value: "abc" });
    component.focus();
    const input = screen.getByRole("searchbox") as HTMLInputElement;
    expect(input).toHaveFocus();
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(3);
  });
});
