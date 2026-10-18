// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import Button from "./Button.svelte";
import IconButton from "./IconButton.svelte";

const text = (s: string) => createRawSnippet(() => ({ render: () => `<span>${s}</span>` }));

describe("Button", () => {
  it("renders a button with the variant class and type=button by default", () => {
    render(Button, { variant: "primary", children: text("Play") });
    const btn = screen.getByRole("button", { name: "Play" });
    expect(btn.tagName).toBe("BUTTON");
    expect(btn).toHaveClass("btn", "btn-primary", "btn-md");
    expect(btn).toHaveAttribute("type", "button");
  });

  it("renders an <a> when href is set", () => {
    render(Button, { href: "https://example.com", children: text("Docs") });
    const link = screen.getByRole("link", { name: "Docs" });
    expect(link.tagName).toBe("A");
    expect(link).toHaveAttribute("href", "https://example.com");
  });

  it("calls onclick when clicked", async () => {
    const onclick = vi.fn();
    render(Button, { onclick, children: text("Go") });
    await userEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onclick).toHaveBeenCalledOnce();
  });

  it("loading sets aria-busy and blocks onclick", async () => {
    const onclick = vi.fn();
    render(Button, { loading: true, onclick, children: text("Save") });
    const btn = screen.getByRole("button", { name: "Save" });
    expect(btn).toHaveAttribute("aria-busy", "true");
    expect(btn).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(btn);
    expect(onclick).not.toHaveBeenCalled();
  });

  it("disabled blocks onclick", async () => {
    const onclick = vi.fn();
    render(Button, { disabled: true, onclick, children: text("Nope") });
    const btn = screen.getByRole("button", { name: "Nope" });
    expect(btn).toBeDisabled();
    await userEvent.click(btn);
    expect(onclick).not.toHaveBeenCalled();
  });
});

describe("IconButton", () => {
  it("takes its accessible name and tooltip from label", () => {
    render(IconButton, { icon: "close", label: "Close panel" });
    const btn = screen.getByRole("button", { name: "Close panel" });
    expect(btn).toHaveAttribute("title", "Close panel");
  });

  it("sets aria-pressed only when pressed is given", () => {
    const { unmount } = render(IconButton, { icon: "favorite", label: "Favorite", pressed: true });
    expect(screen.getByRole("button", { name: "Favorite" })).toHaveAttribute("aria-pressed", "true");
    unmount();
    render(IconButton, { icon: "more", label: "More" });
    expect(screen.getByRole("button", { name: "More" })).not.toHaveAttribute("aria-pressed");
  });

  it("renders aria-pressed=false for an unpressed toggle", () => {
    render(IconButton, { icon: "favorite", label: "Favorite", pressed: false });
    expect(screen.getByRole("button", { name: "Favorite" })).toHaveAttribute("aria-pressed", "false");
  });
});
