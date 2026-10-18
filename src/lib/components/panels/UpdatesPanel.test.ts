// @vitest-environment jsdom
// The Updates view and panel: never-checked empty state, sections in order with empty ones hidden,
// Update all, ModHub open-then-use, Couldn't check with reasons, collapsed Up to date / Not
// tracked, the catalog error, and the auto-check offer shown on the first visit only.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { createRawSnippet } from "svelte";
import UpdatesPanel from "./UpdatesPanel.svelte";
import UpdatesView from "../../views/UpdatesView.svelte";
import { NO_SECTIONS, type UpdateSections } from "../../updates/sections";
import { autoCheckEnabled, reloadAutoCheck } from "../../updates/autoCheck.svelte";
import type { UpdateRow } from "../../types";

const row = (techName: string, extra: Partial<UpdateRow> = {}): UpdateRow => ({
  techName,
  title: techName,
  path: `C:/mods/${techName}.zip`,
  current: "1.0.0.0",
  latest: "1.1.0.0",
  hasUpdate: true,
  ...extra,
});

function setup(sections: Partial<UpdateSections>, props: Record<string, unknown> = {}) {
  const handlers = {
    onCheck: vi.fn(),
    onInstall: vi.fn(),
    onUpdateAll: vi.fn(),
    onOpenPage: vi.fn(),
    onUseDropped: vi.fn(),
    onOpenSource: vi.fn(),
    onMarkChecked: vi.fn(),
    onRecheck: vi.fn(),
    onCopyDetails: vi.fn(),
  };
  render(UpdatesPanel, {
    sections: { ...NO_SECTIONS, ...sections },
    checked: true,
    checking: false,
    catalogError: null,
    rowStates: {},
    updatingAll: false,
    lockReason: "",
    checkDisabledReason: "",
    foreign: [],
    ...handlers,
    ...props,
  });
  return { ...handlers, user: userEvent.setup() };
}

describe("UpdatesPanel", () => {
  it("before any check: the empty state with Check now and the privacy line", async () => {
    const h = setup({}, { checked: false });
    expect(screen.getByText("Check your mods for updates")).toBeInTheDocument();
    expect(screen.getByText(/sends the names of your installed mods/)).toBeInTheDocument();
    await h.user.click(screen.getByRole("button", { name: "Check now" }));
    expect(h.onCheck).toHaveBeenCalledOnce();
  });

  it("shows sections in order and hides empty ones", () => {
    setup({
      ready: [row("A", { assetUrl: "https://x/a.zip", source: "github" })],
      modhub: [row("B", { assetUrl: null, pageUrl: "https://www.farming-simulator.com/mod.php?mod_id=1" })],
      failed: [row("C", { hasUpdate: undefined, error: "GitHub is limiting requests for a while." })],
      upToDate: [row("D", { hasUpdate: false })],
    });
    const headings = screen.getAllByRole("heading").map((h) => h.textContent?.replace(/\s+/g, " ").trim());
    expect(headings).toEqual([
      "Ready to install (1)",
      "Download from ModHub (1)",
      "Couldn't check (1)",
      "Up to date (1)",
    ]);
    expect(screen.queryByText(/Check these yourself/)).toBeNull();
    expect(screen.queryByText(/Not tracked/)).toBeNull();
  });

  it("Update all and per-row Update call back; a failed row offers Try again and Copy details", async () => {
    const a = row("AutoDrive", { assetUrl: "https://x/a.zip" });
    const b = row("Courseplay", { assetUrl: "https://x/b.zip" });
    const h = setup(
      { ready: [a, b] },
      { rowStates: { Courseplay: { phase: "error", message: "The download stopped responding.", detail: "Code: NET_TIMEOUT" } } },
    );
    await h.user.click(screen.getByRole("button", { name: /Update all 2/ }));
    expect(h.onUpdateAll).toHaveBeenCalledOnce();
    await h.user.click(screen.getByRole("button", { name: "Update AutoDrive" }));
    expect(h.onInstall).toHaveBeenCalledWith(a);
    expect(screen.getByText("The download stopped responding.")).toBeInTheDocument();
    await h.user.click(screen.getByRole("button", { name: "Try again: update Courseplay" }));
    expect(h.onInstall).toHaveBeenLastCalledWith(b);
    await h.user.click(screen.getByRole("button", { name: "Copy details: Courseplay" }));
    expect(h.onCopyDetails).toHaveBeenCalledWith("Code: NET_TIMEOUT");
  });

  it("a running install shows in its row and Update all waits", () => {
    setup(
      { ready: [row("A", { assetUrl: "https://x/a.zip" })] },
      { rowStates: { A: { phase: "installing" } }, updatingAll: true },
    );
    expect(screen.getByText("Installing…")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Update all 1/ })).toBeDisabled();
  });

  it("ModHub: opens the page, then offers the saved zip", async () => {
    const hub = row("FS25_Lizard", {
      title: "Lizard R-Series",
      assetUrl: null,
      pageUrl: "https://www.farming-simulator.com/mod.php?mod_id=7",
    });
    const h = setup({ modhub: [hub] });
    expect(screen.queryByRole("button", { name: /Use this version/ })).toBeNull();
    await h.user.click(screen.getByRole("button", { name: "Open Lizard R-Series on ModHub" }));
    expect(h.onOpenPage).toHaveBeenCalledWith(hub);
  });

  it("ModHub: a saved newer zip gets Use this version", async () => {
    const hub = row("FS25_Lizard", { title: "Lizard R-Series", assetUrl: null, pageUrl: null });
    const file = {
      techName: "FS25_Lizard",
      fileName: "FS25_Lizard.zip",
      kind: "zip",
      flatVersion: "1.0.2.0",
      managedVersion: "1.0.0.0",
    };
    const h = setup({ modhub: [hub] }, { foreign: [file] });
    expect(screen.getByText(/Found 1.0.2.0 in your mods folder/)).toBeInTheDocument();
    await h.user.click(screen.getByRole("button", { name: "Use this version of Lizard R-Series" }));
    expect(h.onUseDropped).toHaveBeenCalledWith(file, hub);
  });

  it("Couldn't check lists the reason with Try again", async () => {
    const c = row("C", { hasUpdate: undefined, error: "GitHub is limiting requests for a while.", errorDetail: "d" });
    const h = setup({ failed: [c] });
    expect(screen.getByText("GitHub is limiting requests for a while.")).toBeInTheDocument();
    await h.user.click(screen.getByRole("button", { name: "Try again: check C" }));
    expect(h.onRecheck).toHaveBeenCalledWith(c);
  });

  it("Up to date and Not tracked start collapsed; Not tracked explains itself", () => {
    setup({
      upToDate: [row("D", { hasUpdate: false })],
      notTracked: [{ techName: "E", title: "Some map" }],
    });
    const folds = document.querySelectorAll("details");
    expect(folds).toHaveLength(2);
    folds.forEach((d) => expect(d.open).toBe(false));
    expect(within(folds[1] as HTMLElement).getByText(/save that page in the mod's details/)).toBeInTheDocument();
    expect(screen.getByText(/Every mod Silo can check is up to date/)).toBeInTheDocument();
  });

  it("a catalog that didn't answer says so, with Try again", async () => {
    const h = setup({}, { catalogError: { message: "Silo can't reach the internet right now.", detail: "Code: NET_OFFLINE" } });
    expect(screen.getByText(/Silo can't reach its catalog right now/)).toBeInTheDocument();
    await h.user.click(screen.getByRole("button", { name: "Try again" }));
    expect(h.onCheck).toHaveBeenCalledOnce();
  });
});

describe("UpdatesView auto-check offer", () => {
  const children = createRawSnippet(() => ({ render: () => "<div>rows</div>" }));
  const view = () =>
    render(UpdatesView, { checking: false, checkedAt: null, checkDisabled: false, oncheck: () => {}, children });

  beforeEach(() => {
    localStorage.clear();
    reloadAutoCheck();
  });

  it("is shown on the first visit only, and off unless chosen", () => {
    const first = view();
    expect(screen.getByText("Check for mod updates when Silo starts?")).toBeInTheDocument();
    first.unmount();
    // Not answered, but already offered: a later visit doesn't ask again.
    reloadAutoCheck();
    view();
    expect(screen.queryByText("Check for mod updates when Silo starts?")).toBeNull();
    expect(autoCheckEnabled()).toBe(false);
  });

  it("Check on start turns it on and closes the offer", async () => {
    const user = userEvent.setup();
    view();
    await user.click(screen.getByRole("button", { name: "Check on start" }));
    expect(autoCheckEnabled()).toBe(true);
    expect(screen.queryByText("Check for mod updates when Silo starts?")).toBeNull();
  });

  it("Only when I ask keeps it off", async () => {
    const user = userEvent.setup();
    view();
    await user.click(screen.getByRole("button", { name: "Only when I ask" }));
    expect(autoCheckEnabled()).toBe(false);
    expect(localStorage.getItem("silo.updates.autoCheck")).toBe("off");
  });
});
