// ask()/askText() when the dialog host is gone (#228): before the host first mounts, requests
// wait in the queue (startup); once it has mounted and then gone away (its view boundary caught
// a failure), ask() uses the OS dialog with the same text and askText() gives null with an error
// toast, so nothing waits on a dialog that can't show.
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-dialog", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/plugin-dialog")>()),
  confirm: vi.fn(() => Promise.resolve(true)),
}));
import { confirm as pluginConfirm } from "@tauri-apps/plugin-dialog";
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
    await expect(ask(opts)).resolves.toBe(true);
    expect(currentDialog()).toBeUndefined();
    expect(pluginConfirm).toHaveBeenCalledWith(
      "Delete mod set?\n\nThe mods stay in your library.\n\nMods: 12",
      expect.objectContaining({ okLabel: "Delete", cancelLabel: "Cancel", kind: "warning" }),
    );
  });

  it("a failing OS dialog counts as Cancel", async () => {
    registerDialogHost()();
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(pluginConfirm).mockRejectedValueOnce(new Error("no window"));
    await expect(ask(opts)).resolves.toBe(false);
  });

  it("askText() after the host failed gives null and says so in an error toast", async () => {
    registerDialogHost()();
    await expect(askText({ title: "Rename", label: "Name", confirm: "Rename" })).resolves.toBeNull();
    expect(currentDialog()).toBeUndefined();
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toMatchObject({ tone: "danger", sticky: true, message: 'Silo couldn\'t open the "Rename" dialog.' });
  });
});
