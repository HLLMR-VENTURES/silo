// @vitest-environment jsdom
// ask()/askText() through DialogHost: results, initial focus, focus return, Tab trap, inert page.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import DialogHarness from "./__tests__/DialogHarness.svelte";
import NestedDialogHarness from "./__tests__/NestedDialogHarness.svelte";
import type { ConfirmOptions, PromptOptions } from "./dialogs.svelte";

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
  const dialog = await screen.findByRole("dialog", { name: opts.title });
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
});
afterEach(() => appRoot.remove());

describe("ask()", () => {
  it("renders a labelled modal dialog with body, facts and list", async () => {
    const { dialog } = await openAsk(deleteOpts);
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByText("The mods stay in your library.")).toBeInTheDocument();
    expect(screen.getByText("Mods").tagName).toBe("DT");
    expect(screen.getByText("FS25_B").tagName).toBe("LI");
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
