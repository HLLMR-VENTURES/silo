// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
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
  afterEach(() => {
    vi.useRealTimers();
    document.querySelectorAll("[role='tooltip']").forEach((t) => t.remove());
  });

  const tipOf = (btn: HTMLElement) => document.getElementById(btn.getAttribute("aria-describedby") ?? "");

  it("takes its accessible name from label, with no native title", () => {
    render(IconButton, { icon: "close", label: "Close panel" });
    const btn = screen.getByRole("button", { name: "Close panel" });
    // A native title can linger on screen after its button is removed; the in-app tooltip can't.
    expect(btn).not.toHaveAttribute("title");
  });

  it("shows the label as an in-app tooltip on hover", () => {
    vi.useFakeTimers();
    render(IconButton, { icon: "close", label: "Close panel" });
    const btn = screen.getByRole("button", { name: "Close panel" });
    btn.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    const tip = tipOf(btn);
    expect(tip).toHaveAttribute("role", "tooltip");
    expect(tip).toHaveTextContent("Close panel");
    expect(tip?.hidden).toBe(false);
  });

  it("shows the tooltip on keyboard focus", async () => {
    render(IconButton, { icon: "more", label: "More" });
    const btn = screen.getByRole("button", { name: "More" });
    await userEvent.tab();
    expect(btn).toHaveFocus();
    await waitFor(() => expect(tipOf(btn)).toHaveTextContent("More"));
    expect(tipOf(btn)?.hidden).toBe(false);
  });

  it("removes its tooltip when it unmounts", () => {
    vi.useFakeTimers();
    const { unmount } = render(IconButton, { icon: "close", label: "Delete loadout" });
    const btn = screen.getByRole("button", { name: "Delete loadout" });
    btn.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    expect(document.querySelectorAll("[role='tooltip']")).toHaveLength(1);
    unmount();
    expect(document.querySelectorAll("[role='tooltip']")).toHaveLength(0);
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
