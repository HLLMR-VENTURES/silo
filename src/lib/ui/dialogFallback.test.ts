// ask()/askText() when the dialog host is gone (#228): before the host first mounts, requests
// wait in the queue (startup); once it has mounted and then gone away (its view boundary caught
// a failure), ask() uses the OS dialog with the same text and askText() gives null with an error
// toast, so nothing waits on a dialog that can't show.
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/plugin-dialog")>()),
  confirm: vi.fn(() => Promise.resolve(true)),
  message: vi.fn(() => Promise.resolve("Cancel")),
}));
import { confirm as pluginConfirm, message as pluginMessage } from "@tauri-apps/plugin-dialog";
import {
  ask,
  askText,
  currentDialog,
  registerDialogHost,
  resetDialogHostHistory,
  settleDialog,
} from "./dialogs.svelte";
import { dismissToast, toasts } from "./toasts.svelte";

afterEach(() => {
  resetDialogHostHistory();
  for (const t of [...toasts]) dismissToast(t.id);
  vi.mocked(pluginConfirm).mockClear();
  vi.mocked(pluginMessage).mockClear();
});

const opts = {
  title: "Delete mod set?",
  body: "The mods stay in your library.",
  facts: [{ label: "Mods", value: "12" }],
  confirm: "Delete",
  tone: "danger" as const,
};

describe("ask() without a dialog host", () => {
  it("before the host ever mounts, waits in the queue (no OS dialog)", async () => {
    const pending = ask(opts);
    expect(currentDialog()?.kind).toBe("confirm");
    expect(pluginConfirm).not.toHaveBeenCalled();
    // The host mounts later and the user answers.
    const unregister = registerDialogHost();
    settleDialog(currentDialog()!, true);
    await expect(pending).resolves.toBe(true);
    unregister();
  });

  it("after the host failed (mounted, then gone), asks with the OS dialog and the same text", async () => {
    registerDialogHost()();
    await expect(ask({ ...opts, tone: "neutral" })).resolves.toBe(true);
    expect(currentDialog()).toBeUndefined();
    expect(pluginConfirm).toHaveBeenCalledWith(
      "Delete mod set?\n\nThe mods stay in your library.\n\nMods: 12",
      expect.objectContaining({ okLabel: "Delete", cancelLabel: "Cancel", kind: "info" }),
    );
  });

  it("a failing OS dialog counts as Cancel", async () => {
    registerDialogHost()();
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(pluginConfirm).mockRejectedValueOnce(new Error("no window"));
    await expect(ask({ ...opts, tone: "neutral" })).resolves.toBe(false);
    vi.mocked(pluginMessage).mockRejectedValueOnce(new Error("no window"));
    await expect(ask(opts)).resolves.toBe(false);
  });

  it("a prompt whose Cancel has focus in-app keeps the safe answer as the system dialog's default (#392)", async () => {
    registerDialogHost()();
    const point = {
      title: "Point Farming Simulator 25 at this folder too?",
      confirm: "Update the game's setting",
      cancel: "Only in Silo",
      focus: "cancel" as const,
    };
    // Enter (the first button) or closing the dialog: the safe answer.
    vi.mocked(pluginMessage).mockResolvedValueOnce("Only in Silo");
    await expect(ask(point)).resolves.toBe(false);
    vi.mocked(pluginMessage).mockResolvedValueOnce("Cancel");
    await expect(ask(point)).resolves.toBe(false);
    expect(pluginMessage).toHaveBeenCalledWith(
      "Point Farming Simulator 25 at this folder too?",
      expect.objectContaining({ buttons: { yes: "Only in Silo", no: "Update the game's setting", cancel: "Cancel" } }),
    );
    // Only the verb button says yes.
    vi.mocked(pluginMessage).mockResolvedValueOnce("Update the game's setting");
    await expect(ask(point)).resolves.toBe(true);
    // A danger tone counts too; its safe label "Cancel" gets "Close" for the third button.
    vi.mocked(pluginMessage).mockResolvedValueOnce("Delete");
    await expect(ask(opts)).resolves.toBe(true);
    expect(pluginMessage).toHaveBeenLastCalledWith(
      expect.any(String),
      expect.objectContaining({ buttons: { yes: "Cancel", no: "Delete", cancel: "Close" } }),
    );
    expect(pluginConfirm).not.toHaveBeenCalled();
  });

  it("askText() after the host failed gives null and says so in an error toast", async () => {
    registerDialogHost()();
    await expect(askText({ title: "Rename", label: "Name", confirm: "Rename" })).resolves.toBeNull();
    expect(currentDialog()).toBeUndefined();
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toMatchObject({ tone: "danger", sticky: true, message: "Silo couldn't open the “Rename” dialog." });
  });
});
