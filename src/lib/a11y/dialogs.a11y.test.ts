// @vitest-environment jsdom
// The app's own dialogs under axe (#258), and the keyboard rules for the three that used to be
// hand-rolled overlays (settings editor, mod settings, Browse description): a named modal dialog,
// focus inside on open, Esc closes and calls back.
import { beforeAll, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getConfig: vi.fn(() => Promise.resolve({ "a.b#c": "true", "a.d#e": "2" })),
  setConfig: vi.fn(() => Promise.resolve()),
  getModSettings: vi.fn(() =>
    Promise.resolve([
      {
        name: "settings.xml",
        path: "C:/modSettings/FS25_Mod1/settings.xml",
        raw: "<settings/>",
        fields: [
          { id: 1, label: "Show HUD", kind: "bool", value: "true" },
          { id: 2, label: "Speed limit", kind: "int", value: "40" },
        ],
      },
    ]),
  ),
  ghStatus: vi.fn(() =>
    Promise.resolve({ clientId: null, user: null, builtin: true, canWrite: false, canGist: false }),
  ),
  openExternal: vi.fn(() => Promise.resolve()),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));

import ConfigEditor from "../components/ConfigEditor.svelte";
import ModSettings from "../components/ModSettings.svelte";
import DescModal from "../components/DescModal.svelte";
import ShortcutsDialog from "../components/shell/ShortcutsDialog.svelte";
import WhatsNew from "../firstRun/WhatsNew.svelte";
import OrganizePreview from "../components/OrganizePreview.svelte";
import ShareDialog from "../components/modsets/ShareDialog.svelte";
import type { CfgField } from "../configSchemas";
import type { PlannedMove } from "../types";
import { expectNoAxeViolations } from "./axe";
import { modEntry } from "./fixtures";

beforeAll(() => {
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
});

async function settle() {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await tick();
  }
}

const FIELDS: CfgField[] = [
  { path: "a.b#c", label: "Vertical sync", kind: "bool" },
  { path: "a.d#e", label: "Shadow quality", kind: "select", options: ["1", "2", "3"] },
];

/** Opens `component` from a button (so focus has somewhere to go back to), checks axe, then Esc. */
async function escCloses(open: () => void, name: string, onClose: ReturnType<typeof vi.fn>) {
  const user = userEvent.setup();
  open();
  await settle();
  const dialog = await screen.findByRole("dialog", { name });
  expect(dialog).toHaveAttribute("aria-modal", "true");
  await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
  await expectNoAxeViolations(document.body, { fragment: true });
  await user.keyboard("{Escape}");
  await settle();
  expect(onClose).toHaveBeenCalled();
}

describe("hand-rolled overlays, now dialogs", () => {
  it("settings editor: named, labelled fields, Esc closes", async () => {
    const onClose = vi.fn();
    await escCloses(
      () => render(ConfigEditor, { title: "Configure: game.xml", path: "C:/game.xml", fields: FIELDS, onClose }),
      "Configure: game.xml",
      onClose,
    );
  });

  it("settings editor: every field has a name", async () => {
    render(ConfigEditor, { title: "Configure: game.xml", path: "C:/game.xml", fields: FIELDS, onClose: vi.fn() });
    await settle();
    expect(await screen.findByRole("switch", { name: /Vertical sync/ })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Shadow quality" })).toBeInTheDocument();
  });

  it("mod settings: named, Esc closes", async () => {
    const onClose = vi.fn();
    await escCloses(() => render(ModSettings, { modName: "FS25_Mod1", title: "Mod 1", onClose }), "Mod 1", onClose);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("mod settings: fields named by their labels", async () => {
    render(ModSettings, { modName: "FS25_Mod1", title: "Mod 1", onClose: vi.fn() });
    expect(await screen.findByRole("switch", { name: "Show HUD" })).toBeInTheDocument();
    expect(screen.getByRole("spinbutton", { name: "Speed limit" })).toBeInTheDocument();
  });

  it("Browse description: named, Esc closes", async () => {
    const onClose = vi.fn();
    await escCloses(
      () =>
        render(DescModal, {
          modal: { title: "Lizard Pack", text: "A pack.", url: "https://example.invalid", loading: false, source: "catalog" },
          onClose,
        }),
      "Lizard Pack",
      onClose,
    );
  });
});

describe("axe: app dialogs", () => {
  it("keyboard shortcuts", async () => {
    render(ShortcutsDialog, { open: true });
    await settle();
    await screen.findByRole("dialog");
    await expectNoAxeViolations(document.body, { fragment: true });
  });

  it("what's new", async () => {
    render(WhatsNew, { open: true });
    await settle();
    await screen.findByRole("dialog");
    await expectNoAxeViolations(document.body, { fragment: true });
  });

  it("organize", async () => {
    const mv = (techName: string, category: string): PlannedMove => ({
      techName,
      fileName: `${techName}.zip`,
      category,
      relFrom: `${techName}.zip`,
      relTo: `archive/${category}/${techName}.zip`,
    });
    render(OrganizePreview, {
      plan: [mv("FS25_A", "Tractors"), mv("FS25_B", "Maps")],
      loading: false,
      applying: false,
      activeCount: 3,
      onConfirm: vi.fn(),
      onCancel: vi.fn(),
      onCopyDetails: vi.fn(),
    });
    await settle();
    await screen.findByRole("dialog");
    await expectNoAxeViolations(document.body, { fragment: true });
  });

  it("share a set", async () => {
    render(ShareDialog, {
      open: true,
      name: "Harvest 2026",
      mods: ["FS25_Mod1", "FS25_Mod2"],
      library: [modEntry(1), modEntry(2)],
      onConnect: vi.fn(),
      onShared: vi.fn(),
    });
    await settle();
    await screen.findByRole("dialog");
    await expectNoAxeViolations(document.body, { fragment: true });
  });
});
