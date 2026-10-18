// @vitest-environment jsdom
// ask()/askText() through DialogHost: results, initial focus, focus return, Tab trap, inert page,
// the arm delay against carried-over clicks and key repeat, and cleanup on unmount.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import Dialog from "./Dialog.svelte";
import DialogHost from "./DialogHost.svelte";
import DialogHarness from "./__tests__/DialogHarness.svelte";
import NestedDialogHarness from "./__tests__/NestedDialogHarness.svelte";
import {
  ARM_MS,
  ask,
  askText,
  setDialogArmDelay,
  type ConfirmOptions,
  type PromptOptions,
} from "./dialogs.svelte";

const deleteOpts: ConfirmOptions = {
  title: "Delete mod set?",
  body: "The mods stay in your library.",
  facts: [{ label: "Mods", value: "12" }],
  list: ["FS25_A", "FS25_B"],
  confirm: "Delete",
  tone: "danger",
};
const moveOpts: ConfirmOptions = { title: "Move mods?", confirm: "Move 12 mods" };

async function openAsk(opts: ConfirmOptions) {
  const user = userEvent.setup();
  const onresult = vi.fn();
  render(DialogHarness, { mode: "ask", opts, onresult });
  const opener = screen.getByRole("button", { name: "Open dialog" });
  await user.click(opener);
  const role = opts.tone === "danger" ? "alertdialog" : "dialog";
  const dialog = await screen.findByRole(role, { name: opts.title });
  return { user, onresult, opener, dialog };
}

async function openPrompt(opts: PromptOptions) {
  const user = userEvent.setup();
  const onresult = vi.fn();
  render(DialogHarness, { mode: "askText", opts, onresult });
  await user.click(screen.getByRole("button", { name: "Open dialog" }));
  const dialog = await screen.findByRole("dialog", { name: opts.title });
  const input = screen.getByRole("textbox", { name: opts.label });
  await waitFor(() => expect(input).toHaveFocus());
  return { user, onresult, dialog, input };
}

let appRoot: HTMLDivElement;
beforeEach(() => {
  appRoot = document.createElement("div");
  appRoot.id = "app";
  document.body.prepend(appRoot);
  // Most tests click the verb button at once; the arm delay has its own tests below.
  setDialogArmDelay(0);
});
afterEach(() => {
  appRoot.remove();
  setDialogArmDelay();
  vi.useRealTimers();
});

/** No element anywhere in the document is inert. */
function nothingInert() {
  expect(document.querySelectorAll("[inert]")).toHaveLength(0);
}

describe("ask()", () => {
  it("renders a labelled modal dialog with body, facts and list", async () => {
    const { dialog } = await openAsk(deleteOpts);
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByText("The mods stay in your library.")).toBeInTheDocument();
    expect(screen.getByText("Mods").tagName).toBe("DT");
    expect(screen.getByText("FS25_B").tagName).toBe("LI");
  });

  it("danger confirms are alertdialogs, described by their body", async () => {
    const { dialog } = await openAsk(deleteOpts);
    expect(dialog).toHaveAttribute("role", "alertdialog");
    expect(dialog).toHaveAccessibleDescription("The mods stay in your library.");
  });

  it("neutral confirms are plain dialogs", async () => {
    const { dialog } = await openAsk({ ...moveOpts, body: "Nothing is deleted." });
    expect(dialog).toHaveAttribute("role", "dialog");
    expect(dialog).toHaveAccessibleDescription("Nothing is deleted.");
  });

  it("resolves false on Cancel", async () => {
    const { user, onresult } = await openAsk(deleteOpts);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(false));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("resolves false on Esc", async () => {
    const { user, onresult } = await openAsk(moveOpts);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(false));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("ignores Esc while an IME is composing", async () => {
    const { onresult, dialog } = await openAsk(moveOpts);
    await fireEvent.keyDown(dialog, { key: "Escape", isComposing: true });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(onresult).not.toHaveBeenCalled();
  });

  it("resolves false on a scrim click", async () => {
    const { onresult, dialog } = await openAsk(moveOpts);
    await fireEvent.click(dialog.parentElement!.querySelector(".scrim")!);
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(false));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("resolves true on the verb button", async () => {
    const { user, onresult } = await openAsk(moveOpts);
    await user.click(screen.getByRole("button", { name: "Move 12 mods" }));
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(true));
    expect(onresult).toHaveBeenCalledTimes(1);
  });

  it("danger tone uses a danger button and focuses Cancel", async () => {
    await openAsk(deleteOpts);
    expect(screen.getByRole("button", { name: "Delete" })).toHaveClass("btn-danger");
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus());
  });

  it("neutral tone uses a primary button and focuses it", async () => {
    await openAsk(moveOpts);
    const verb = screen.getByRole("button", { name: "Move 12 mods" });
    expect(verb).toHaveClass("btn-primary");
    await waitFor(() => expect(verb).toHaveFocus());
  });

  it("returns focus to the opener after closing", async () => {
    const { user, opener } = await openAsk(moveOpts);
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("traps Tab: the last button wraps to the first and back", async () => {
    const { user } = await openAsk(moveOpts);
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const verb = screen.getByRole("button", { name: "Move 12 mods" });
    await waitFor(() => expect(verb).toHaveFocus());
    await user.tab();
    expect(cancel).toHaveFocus();
    await user.tab({ shift: true });
    expect(verb).toHaveFocus();
  });

  it("makes #app and the other body children inert while open, and restores them", async () => {
    const { user, opener, dialog } = await openAsk(moveOpts);
    const layer = dialog.parentElement!;
    expect(layer.parentElement).toBe(document.body);
    expect(appRoot).toHaveAttribute("inert");
    for (const child of Array.from(document.body.children)) {
      if (child !== layer) expect(child).toHaveAttribute("inert");
    }
    expect(layer).not.toHaveAttribute("inert");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(appRoot).not.toHaveAttribute("inert");
    expect(opener.closest("[inert]")).toBeNull();
  });

  it("shows queued requests one at a time", async () => {
    const { user, onresult } = await openAsk(moveOpts);
    // A second click is impossible (the page is inert), so queue another request directly.
    const { ask } = await import("./dialogs.svelte");
    const second = ask({ title: "Second?", confirm: "Go" });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Move 12 mods" }));
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(true));
    await screen.findByRole("dialog", { name: "Second?" });
    await user.keyboard("{Escape}");
    await expect(second).resolves.toBe(false);
  });

  it("cancelling one request leaves the next one pending, and it still works", async () => {
    const user = userEvent.setup();
    render(DialogHost);
    const first = ask(moveOpts);
    const second = ask({ title: "Second?", confirm: "Go" });
    await screen.findByRole("dialog", { name: "Move mods?" });
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    await expect(first).resolves.toBe(false);
    await screen.findByRole("dialog", { name: "Second?" });
    await user.click(screen.getByRole("button", { name: "Go" }));
    await expect(second).resolves.toBe(true);
  });

  it("a repeating Enter or Space on the verb button doesn't confirm", async () => {
    const { onresult } = await openAsk(moveOpts);
    const verb = screen.getByRole("button", { name: "Move 12 mods" });
    // fireEvent returns false when the handler prevented the default (the button's activation).
    expect(await fireEvent.keyDown(verb, { key: "Enter", repeat: true })).toBe(false);
    expect(await fireEvent.keyDown(verb, { key: " ", repeat: true })).toBe(false);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(onresult).not.toHaveBeenCalled();
  });

  it("holding Enter on one confirm doesn't confirm the next in the queue", async () => {
    const { user, onresult } = await openAsk(moveOpts);
    const second = ask({ title: "Second?", confirm: "Go" });
    let settled = false;
    void second.then(() => (settled = true));
    await waitFor(() => expect(screen.getByRole("button", { name: "Move 12 mods" })).toHaveFocus());
    await user.keyboard("{Enter>5}");
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(true));
    await screen.findByRole("dialog", { name: "Second?" });
    await user.keyboard("{/Enter}");
    await tick();
    expect(settled).toBe(false);
    await user.keyboard("{Escape}");
    await expect(second).resolves.toBe(false);
  });
});

describe("arm delay", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    setDialogArmDelay(ARM_MS);
  });

  it("is 300 ms by default", () => {
    expect(ARM_MS).toBe(300);
  });

  it("a fast second click lands on the next dialog without confirming it", async () => {
    render(DialogHost);
    const first = ask(moveOpts);
    const second = ask({ title: "Second?", confirm: "Go" });
    let secondDone = false;
    void second.then(() => (secondDone = true));
    await tick();
    const verb = screen.getByRole("button", { name: "Move 12 mods" });
    await fireEvent.click(verb); // straight after opening: ignored
    expect(screen.getByRole("dialog", { name: "Move mods?" })).toBeInTheDocument();
    await act(() => vi.advanceTimersByTime(ARM_MS));
    await fireEvent.click(verb); // the double-click's first click
    await expect(first).resolves.toBe(true);
    await tick();
    const go = screen.getByRole("button", { name: "Go" });
    await fireEvent.click(go); // ...and its second, on the next dialog's button
    await tick();
    expect(screen.getByRole("dialog", { name: "Second?" })).toBeInTheDocument();
    expect(secondDone).toBe(false);
    await act(() => vi.advanceTimersByTime(ARM_MS - 1));
    await fireEvent.click(go);
    expect(secondDone).toBe(false);
    await act(() => vi.advanceTimersByTime(1));
    await fireEvent.click(go);
    await expect(second).resolves.toBe(true);
  });

  it("Cancel works at once", async () => {
    render(DialogHost);
    const pending = ask(moveOpts);
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await expect(pending).resolves.toBe(false);
  });

  it("a prompt's Replace needs another full delay after the warning", async () => {
    render(DialogHost);
    const validate = (v: string) => (v === "Taken" ? { warn: "Exists.", confirm: "Replace" } : null);
    const pending = askText({ title: "Save", label: "Name", value: "Taken", confirm: "Save", validate });
    let done = false;
    void pending.then(() => (done = true));
    await tick();
    const input = screen.getByRole("textbox", { name: "Name" });
    await fireEvent.keyDown(input, { key: "Enter" }); // too early: nothing happens
    expect(screen.queryByText("Exists.")).not.toBeInTheDocument();
    await act(() => vi.advanceTimersByTime(ARM_MS));
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(screen.getByText("Exists.")).toBeInTheDocument();
    const replace = screen.getByRole("button", { name: "Replace" });
    await fireEvent.keyDown(input, { key: "Enter" }); // a double Enter
    await fireEvent.click(replace);
    expect(done).toBe(false);
    await act(() => vi.advanceTimersByTime(ARM_MS));
    await fireEvent.click(replace);
    await expect(pending).resolves.toBe("Taken");
  });
});

describe("cleanup", () => {
  it("unmounting the host resolves pending requests and leaves nothing inert", async () => {
    const { unmount } = render(DialogHost);
    const pendingAsk = ask(moveOpts);
    const pendingText = askText({ title: "Name it", label: "Name", confirm: "Save" });
    await screen.findByRole("dialog", { name: "Move mods?" });
    expect(appRoot).toHaveAttribute("inert");
    unmount();
    await expect(pendingAsk).resolves.toBe(false);
    await expect(pendingText).resolves.toBeNull();
    for (const child of Array.from(document.body.children)) expect(child).not.toHaveAttribute("inert");
    nothingInert();
  });

  it("a Dialog unmounted while open clears inert", async () => {
    const { unmount } = render(Dialog, { open: true, title: "Solo" });
    await screen.findByRole("dialog", { name: "Solo" });
    expect(appRoot).toHaveAttribute("inert");
    unmount();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    nothingInert();
  });
});

describe("nested dialogs", () => {
  it("only the top dialog is interactive, and Esc closes only the top", async () => {
    const user = userEvent.setup();
    render(NestedDialogHarness);
    await user.click(screen.getByRole("button", { name: "Open outer" }));
    const outer = await screen.findByRole("dialog", { name: "Outer" });
    const openInner = screen.getByRole("button", { name: "Open inner" });
    await waitFor(() => expect(openInner).toHaveFocus());
    await user.click(openInner);
    const inner = await screen.findByRole("dialog", { name: "Inner" });
    await waitFor(() => expect(screen.getByRole("button", { name: "Inner action" })).toHaveFocus());
    expect(outer.parentElement).toHaveAttribute("inert");
    expect(inner.parentElement).not.toHaveAttribute("inert");

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Inner" })).not.toBeInTheDocument());
    expect(screen.getByRole("dialog", { name: "Outer" })).toBeInTheDocument();
    expect(outer.parentElement).not.toHaveAttribute("inert");
    expect(appRoot).toHaveAttribute("inert");
    await waitFor(() => expect(openInner).toHaveFocus());

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(appRoot).not.toHaveAttribute("inert");
    await waitFor(() => expect(screen.getByRole("button", { name: "Open outer" })).toHaveFocus());
  });
});

describe("askText()", () => {
  const rename: PromptOptions = { title: "Rename mod set", label: "Name", value: "Old", confirm: "Rename" };

  it("focuses the field with its text selected", async () => {
    const { input } = await openPrompt(rename);
    const el = input as HTMLInputElement;
    expect(el.value).toBe("Old");
    expect(el.selectionStart).toBe(0);
    expect(el.selectionEnd).toBe(3);
  });

  it("returns the trimmed text on Enter", async () => {
    const { user, onresult, input } = await openPrompt(rename);
    await user.clear(input);
    await user.type(input, "  Harvest set  {Enter}");
    await waitFor(() => expect(onresult).toHaveBeenCalledWith("Harvest set"));
  });

  it("a repeating Enter doesn't submit", async () => {
    const { onresult, input } = await openPrompt(rename);
    await fireEvent.keyDown(input, { key: "Enter", repeat: true });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(onresult).not.toHaveBeenCalled();
    await fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => expect(onresult).toHaveBeenCalledWith("Old"));
  });

  it("returns null on Esc", async () => {
    const { user, onresult } = await openPrompt(rename);
    await user.keyboard("{Escape}");
    await waitFor(() => expect(onresult).toHaveBeenCalledWith(null));
  });

  it("blocks on a validation error and shows the message", async () => {
    const validate = (v: string) => (v ? null : "Name can't be empty");
    const { user, onresult, input } = await openPrompt({ ...rename, validate });
    await user.clear(input);
    await user.keyboard("{Enter}");
    expect(screen.getByText("Name can't be empty")).toBeInTheDocument();
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(onresult).not.toHaveBeenCalled();
    await user.type(input, "Fixed{Enter}");
    await waitFor(() => expect(onresult).toHaveBeenCalledWith("Fixed"));
  });

  it("{error} objects block the same way", async () => {
    const { user, onresult } = await openPrompt({ ...rename, validate: () => ({ error: "Nope" }) });
    await user.keyboard("{Enter}");
    expect(screen.getByText("Nope")).toBeInTheDocument();
    expect(onresult).not.toHaveBeenCalled();
  });

  it("warn relabels the button and accepts on the second submit", async () => {
    const validate = (v: string) =>
      v === "Taken" ? { warn: "A mod set named Taken exists.", confirm: "Replace" } : null;
    const { user, onresult, input } = await openPrompt({ ...rename, validate });
    await user.clear(input);
    await user.type(input, "Taken{Enter}");
    expect(screen.getByText("A mod set named Taken exists.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Replace" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Rename" })).not.toBeInTheDocument();
    expect(onresult).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Replace" }));
    await waitFor(() => expect(onresult).toHaveBeenCalledWith("Taken"));
  });

  it("editing the value after a warning clears it", async () => {
    const validate = (v: string) => (v === "Taken" ? { warn: "Exists.", confirm: "Replace" } : null);
    const { user, input } = await openPrompt({ ...rename, value: "Taken", validate });
    await user.keyboard("{Enter}");
    expect(screen.getByRole("button", { name: "Replace" })).toBeInTheDocument();
    await user.type(input, "2");
    expect(screen.queryByText("Exists.")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rename" })).toBeInTheDocument();
  });
});
