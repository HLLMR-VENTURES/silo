// @vitest-environment jsdom
// Mod sets page (#251): Open a shared set only previews (nothing downloads, saves or switches
// without a click), a Multiplayer check file is checked on click, and the one Share dialog's
// three ways (link, .silo file, Multiplayer check file).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { AppError } from "../../errors";
import { tick } from "svelte";

vi.mock("../../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api")>()),
  collectionImportPreview: vi.fn(),
  collectionApply: vi.fn(),
  onCollectionProgress: vi.fn(() => Promise.resolve(() => {})),
  detectConflicts: vi.fn(() => Promise.resolve([])),
  mpVerifyPath: vi.fn(),
  pickSharedSetFile: vi.fn(),
  sharedFileKindOf: vi.fn(() => Promise.reject(new Error("no backend"))),
  collectionExport: vi.fn(),
  exportSetFile: vi.fn(() => Promise.resolve(true)),
  mpExport: vi.fn(() => Promise.resolve({ count: 2, unreadable: [] })),
  ghStatus: vi.fn(),
  openExternal: vi.fn(() => Promise.resolve()),
  copyText: vi.fn(() => Promise.resolve()),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));

import {
  collectionApply,
  collectionExport,
  collectionImportPreview,
  exportSetFile,
  ghStatus,
  mpExport,
  mpVerifyPath,
  pickSharedSetFile,
  sharedFileKindOf,
} from "../../api";
import type { ImportPlan, ModEntry } from "../../types";
import OpenSharedTab from "./OpenSharedTab.svelte";
import ShareDialog from "./ShareDialog.svelte";

const mod = (techName: string, kind = "zip"): ModEntry =>
  ({ techName, title: `Title ${techName}`, version: "1.0", path: `/mods/${techName}.zip`, kind, dependencies: [] }) as unknown as ModEntry;
const library = [mod("A"), mod("B"), mod("Dev", "dir")];

const plan: ImportPlan = {
  name: "Weekend pack",
  description: null,
  author: "farmer",
  willInstall: [{ techName: "N", version: "2.0", source: "github", installedVersion: null }],
  openPage: [{ techName: "H", version: null, source: "modhub", installedVersion: null }],
  alreadyPresent: [{ techName: "A", version: "1.0", source: null, installedVersion: "1.0" }],
  versionDrift: [],
  unresolved: [{ techName: "U", version: null, source: null, installedVersion: null }],
  fingerprint: "fp1",
};

// Deep-link requests are remembered across mounts (by `n`), so every test uses a new one.
let reqN = 0;
const req = (url: string) => ({ url, n: ++reqN });

function openTab(extra: Record<string, unknown> = {}) {
  const props = {
    library,
    active: new Set(["A", "B"]),
    activeRefs: library.slice(0, 2).map((m) => ({ techName: m.techName, path: m.path, kind: m.kind, version: m.version })),
    onSaveAsSet: vi.fn(() => Promise.resolve(77)),
    onSwitchTo: vi.fn(),
    onImported: vi.fn(),
    onTurnOn: vi.fn(),
    onPark: vi.fn(),
    onFindInBrowse: vi.fn(),
    ...extra,
  };
  const r = render(OpenSharedTab, props);
  return Object.assign(props, { rerender: r.rerender, unmount: r.unmount });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(collectionImportPreview).mockResolvedValue(plan);
});

describe("Open a shared set", () => {
  it("a pasted link only previews: nothing downloads, saves or switches", async () => {
    const p = openTab();
    await fireEvent.input(screen.getByLabelText("Paste a link, or choose a file"), {
      target: { value: " https://gist.github.com/u/abc " },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(await screen.findByRole("heading", { name: "Weekend pack" })).toBeInTheDocument();
    expect(collectionImportPreview).toHaveBeenCalledWith(
      "https://gist.github.com/u/abc",
      expect.arrayContaining([{ techName: "A", version: "1.0" }]),
      null,
    );
    expect(screen.getByText("Silo can download (1)")).toBeInTheDocument();
    expect(screen.getByText("Get from ModHub (1)")).toBeInTheDocument();
    expect(screen.getByText("Couldn't find (1)")).toBeInTheDocument();
    expect(screen.getByText("Already in your library (1)")).toBeInTheDocument();
    expect(collectionApply).not.toHaveBeenCalled();
    expect(p.onSaveAsSet).not.toHaveBeenCalled();
    expect(p.onSwitchTo).not.toHaveBeenCalled();
  });

  it("a deep link request previews at once, and still applies nothing", async () => {
    const p = openTab({ request: req("https://gist.github.com/u/abc") });
    expect(await screen.findByRole("heading", { name: "Weekend pack" })).toBeInTheDocument();
    expect(collectionApply).not.toHaveBeenCalled();
    expect(p.onSaveAsSet).not.toHaveBeenCalled();
  });

  it("Save as a set without downloading saves every listed mod, then offers Switch to it", async () => {
    const p = openTab({ request: req("x") });
    await fireEvent.click(await screen.findByRole("button", { name: "Save as a set without downloading" }));
    await waitFor(() => expect(p.onSaveAsSet).toHaveBeenCalledWith("Weekend pack", ["A", "N", "H", "U"]));
    expect(collectionApply).not.toHaveBeenCalled();
    await fireEvent.click(await screen.findByRole("button", { name: "Switch to it" }));
    expect(p.onSwitchTo).toHaveBeenCalledWith(77);
  });

  it("Download installs exactly what was previewed (its fingerprint)", async () => {
    vi.mocked(collectionApply).mockResolvedValue({ loadoutId: 5, installed: 1, failed: 0, rows: [] });
    const p = openTab({ request: req("https://gist.github.com/u/abc") });
    await fireEvent.click(await screen.findByRole("button", { name: "Download 1 mod and save as a set" }));
    await waitFor(() => expect(p.onImported).toHaveBeenCalled());
    expect(collectionApply).toHaveBeenCalledWith("https://gist.github.com/u/abc", expect.any(Array), "fp1", null);
    expect(screen.getByText(/Saved Weekend pack as a set\. Downloaded 1 mod/)).toBeInTheDocument();
  });

  it("a .silo file previews through the same path, from the file", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("C:\\Users\\me\\weekend.silo");
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    expect(await screen.findByRole("heading", { name: "Weekend pack" })).toBeInTheDocument();
    expect(collectionImportPreview).toHaveBeenCalledWith("", expect.any(Array), "C:\\Users\\me\\weekend.silo");
  });

  it("a Multiplayer check file is checked only on click, with fixes per row", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("/home/me/server.silomp");
    vi.mocked(mpVerifyPath).mockResolvedValue({
      ok: false,
      missing: [
        { techName: "Dev", version: "1", hash: "h", size: 1 },
        { techName: "Q", version: "1", hash: "h", size: 1 },
      ],
      versionMismatch: [],
      hashMismatch: [],
      unverifiable: [],
      extra: ["B"],
    });
    const p = openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    expect(await screen.findByText("server.silomp")).toBeInTheDocument();
    expect(collectionImportPreview).not.toHaveBeenCalled();
    expect(mpVerifyPath).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Check my mods" }));
    expect(await screen.findByText(/Your mods don't match yet/)).toBeInTheDocument();
    expect(mpVerifyPath).toHaveBeenCalledWith("/home/me/server.silomp", expect.any(Array));
    await fireEvent.click(screen.getByRole("button", { name: /^Turn on: / }));
    expect(p.onTurnOn).toHaveBeenCalledWith("Dev");
    await fireEvent.click(screen.getByRole("button", { name: /^Find in Browse: / }));
    expect(p.onFindInBrowse).toHaveBeenCalledWith("Q");
    await fireEvent.click(screen.getByRole("button", { name: /^Park: / }));
    expect(p.onPark).toHaveBeenCalledWith("B");
    expect(screen.getByRole("button", { name: "Check again" })).toBeInTheDocument();
  });

  it("says you match when you do", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("server.silomp");
    vi.mocked(mpVerifyPath).mockResolvedValue({ ok: true, missing: [], versionMismatch: [], hashMismatch: [], unverifiable: [], extra: [] });
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Check my mods" }));
    expect(await screen.findByText(/You match\. You're ready to join\./)).toBeInTheDocument();
  });

  it("names versions that differ, and calls a same-version mismatch a different file (never its hash)", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("server.silomp");
    vi.mocked(mpVerifyPath).mockResolvedValue({
      ok: false,
      missing: [],
      versionMismatch: [{ techName: "A", expected: "1.1.0.0", got: "1.0.0.0" }],
      hashMismatch: [{ techName: "Dev", expected: "aaaa1111", got: "bbbb2222" }],
      unverifiable: [],
      extra: [],
    });
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Check my mods" }));
    expect(await screen.findByText("yours 1.0.0.0, server 1.1.0.0")).toBeInTheDocument();
    expect(screen.getByText("a different file")).toBeInTheDocument();
    expect(screen.queryByText(/aaaa1111|bbbb2222/)).toBeNull();
  });

  it("a mod with no fingerprint on either side is 'can't check', never a match (#355)", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("server.silomp");
    vi.mocked(mpVerifyPath).mockResolvedValue({
      ok: false,
      missing: [],
      versionMismatch: [],
      hashMismatch: [],
      unverifiable: [
        { techName: "A", server: true, yours: false },
        { techName: "B", server: false, yours: true },
      ],
      extra: [],
    });
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Check my mods" }));
    expect(await screen.findByText("Silo can't check these (2)")).toBeInTheDocument();
    expect(screen.getByText("the check file has no fingerprint for it")).toBeInTheDocument();
    expect(screen.getByText("Silo couldn't read your file")).toBeInTheDocument();
    expect(screen.queryByText(/You match/)).toBeNull();
  });

  it("only unverifiable mods: not a match, and not called a mismatch either", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("server.silomp");
    vi.mocked(mpVerifyPath).mockResolvedValue({
      ok: false,
      missing: [],
      versionMismatch: [],
      hashMismatch: [],
      unverifiable: [{ techName: "A", server: true, yours: false }],
      extra: [],
    });
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Check my mods" }));
    expect(await screen.findByText(/Silo couldn't confirm that you match/)).toBeInTheDocument();
    expect(screen.queryByText(/don't match yet/)).toBeNull();
    expect(screen.queryByText(/You match\./)).toBeNull();
  });
});

describe("Open a shared set: races and repeats", () => {
  it("a deep link that lands during a download waits, and the download's result isn't shown on it", async () => {
    let finish!: (r: unknown) => void;
    vi.mocked(collectionApply).mockReturnValue(new Promise((res) => (finish = res)) as never);
    const p = openTab({ request: req("https://gist.github.com/u/a") });
    await fireEvent.click(await screen.findByRole("button", { name: "Download 1 mod and save as a set" }));
    expect(screen.getByRole("button", { name: "Choose a file…" })).toBeDisabled();
    vi.mocked(collectionImportPreview).mockResolvedValue({ ...plan, name: "Other pack" });
    await p.rerender({ request: req("https://gist.github.com/u/b") });
    await tick();
    expect(collectionImportPreview).toHaveBeenCalledTimes(1);
    finish({ loadoutId: 5, installed: 1, failed: 0, rows: [] });
    expect(await screen.findByRole("heading", { name: "Other pack" })).toBeInTheDocument();
    expect(collectionImportPreview).toHaveBeenLastCalledWith("https://gist.github.com/u/b", expect.any(Array), null);
    expect(screen.queryByText(/Saved .* as a set\. Downloaded/)).toBeNull();
    expect(screen.queryByRole("button", { name: "Switch to it" })).toBeNull();
    expect(p.onImported).toHaveBeenCalled();
  });

  it("a deep link from a cold start previews again once the library is read", async () => {
    const r = req("https://gist.github.com/u/cold");
    const p = openTab({ library: [], request: r });
    await screen.findByRole("heading", { name: "Weekend pack" });
    expect(collectionImportPreview).toHaveBeenLastCalledWith(r.url, [], null);
    await p.rerender({ library, request: r });
    await waitFor(() => expect(collectionImportPreview).toHaveBeenCalledTimes(2));
    expect(vi.mocked(collectionImportPreview).mock.calls[1][1]).toHaveLength(3);
    await p.rerender({ library: [...library, mod("C")], request: r });
    await tick();
    expect(collectionImportPreview).toHaveBeenCalledTimes(2);
  });

  it("coming back to the tab doesn't preview the same deep link again", async () => {
    const r = req("https://gist.github.com/u/again");
    const p = openTab({ request: r });
    await screen.findByRole("heading", { name: "Weekend pack" });
    p.unmount();
    openTab({ request: r });
    await tick();
    expect(collectionImportPreview).toHaveBeenCalledTimes(1);
  });

  it("a double click saves the set once", async () => {
    let done!: (v: number) => void;
    const onSaveAsSet = vi.fn(() => new Promise<number>((res) => (done = res)));
    openTab({ request: req("x"), onSaveAsSet });
    const btn = await screen.findByRole("button", { name: "Save as a set without downloading" });
    await fireEvent.click(btn);
    await fireEvent.click(btn);
    done(9);
    await screen.findByRole("button", { name: "Switch to it" });
    expect(onSaveAsSet).toHaveBeenCalledTimes(1);
  });

  it("a set that changed since the preview offers Preview again", async () => {
    vi.mocked(collectionApply).mockRejectedValue(
      new AppError("CHANGED_SINCE_PREVIEW", "This set changed since you previewed it. Preview it again before installing."),
    );
    openTab({ request: req("https://gist.github.com/u/changed") });
    await fireEvent.click(await screen.findByRole("button", { name: "Download 1 mod and save as a set" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Preview again" }));
    await waitFor(() => expect(collectionImportPreview).toHaveBeenCalledTimes(2));
  });

  it("a check that finishes after another file was chosen isn't shown under it (#355)", async () => {
    let finish!: (r: unknown) => void;
    vi.mocked(mpVerifyPath).mockReturnValueOnce(new Promise((res) => (finish = res)) as never);
    vi.mocked(pickSharedSetFile).mockResolvedValueOnce("/x/serverA.silomp");
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Check my mods" }));
    vi.mocked(pickSharedSetFile).mockResolvedValueOnce("/x/serverB.silomp");
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    expect(await screen.findByText("serverB.silomp")).toBeInTheDocument();
    finish({ ok: true, missing: [], versionMismatch: [], hashMismatch: [], unverifiable: [], extra: [] });
    await tick();
    await waitFor(() => expect(screen.getByRole("button", { name: "Check my mods" })).toBeEnabled());
    expect(screen.queryByText(/You match/)).toBeNull();
  });

  it("a Multiplayer check file saved as .json still opens as one", async () => {
    vi.mocked(pickSharedSetFile).mockResolvedValue("/x/server.json");
    vi.mocked(sharedFileKindOf).mockResolvedValueOnce("multiplayer");
    openTab();
    await fireEvent.click(screen.getByRole("button", { name: "Choose a file…" }));
    expect(await screen.findByRole("button", { name: "Check my mods" })).toBeInTheDocument();
    expect(collectionImportPreview).not.toHaveBeenCalled();
  });
});

describe("Share dialog", () => {
  const gh = (canGist: boolean) => ({ clientId: null, user: null, builtin: true, canWrite: false, canGist });

  function share(extra: Record<string, unknown> = {}) {
    const props = {
      open: true,
      name: "Harvest 2026",
      mods: ["A", "Dev", "Gone"],
      library,
      onConnect: vi.fn(),
      onShared: vi.fn(),
      ...extra,
    };
    const r = render(ShareDialog, props);
    return Object.assign(props, { rerender: r.rerender });
  }

  it("a link needs GitHub: Connect GitHub when it isn't connected", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(false));
    const p = share();
    expect(await screen.findByRole("dialog", { name: "Share “Harvest 2026”" })).toBeInTheDocument();
    await fireEvent.click(await screen.findByRole("button", { name: "Connect GitHub" }));
    expect(p.onConnect).toHaveBeenCalled();
    expect(collectionExport).not.toHaveBeenCalled();
  });

  it("creates an unlisted link from the set's mods Silo has files for", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(true));
    vi.mocked(collectionExport).mockResolvedValue({
      url: "https://silo.hllmr.com/c/?g=1",
      count: 1,
      omitted: ["Dev"],
      unpinned: [],
      warning: null,
    });
    const p = share();
    await fireEvent.click(await screen.findByRole("button", { name: "Create link" }));
    await waitFor(() => expect(collectionExport).toHaveBeenCalled());
    const [name, desc, isPublic, refs] = vi.mocked(collectionExport).mock.calls[0];
    expect([name, desc, isPublic]).toEqual(["Harvest 2026", null, false]);
    expect(refs.map((r) => r.techName)).toEqual(["A", "Dev"]);
    expect(await screen.findByText("https://silo.hllmr.com/c/?g=1")).toBeInTheDocument();
    expect(p.onShared).toHaveBeenCalled();
    expect(screen.getByText(/1 mod in this set isn't in your library/)).toBeInTheDocument();
  });

  it("a .silo file needs no account", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(false));
    share();
    await fireEvent.click(await screen.findByLabelText(/^File/));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Save file…" }));
    await waitFor(() => expect(exportSetFile).toHaveBeenCalledWith("Harvest 2026", ["A", "Dev", "Gone"]));
  });

  it("a Multiplayer check file fingerprints the mods Silo has", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(false));
    share();
    await fireEvent.click(await screen.findByLabelText(/^Multiplayer check file/));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Save file…" }));
    await waitFor(() => expect(mpExport).toHaveBeenCalled());
    const [refs, name] = vi.mocked(mpExport).mock.calls[0];
    expect(refs.map((r) => r.techName)).toEqual(["A", "Dev"]);
    expect(name).toBe("Harvest 2026");
  });

  it("a check file names the mods Silo couldn't read (#355)", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(false));
    vi.mocked(mpExport).mockResolvedValueOnce({ count: 2, unreadable: ["A"] });
    share();
    await fireEvent.click(await screen.findByLabelText(/^Multiplayer check file/));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Save file…" }));
    expect(await screen.findByText(/Saved a multiplayer check file for 2 mods/)).toBeInTheDocument();
    expect(screen.getByText(/Silo couldn't read 1 mod \(Title A\)/)).toBeInTheDocument();
  });

  it("saving the check file again and cancelling doesn't leave the old note without its warning", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(false));
    vi.mocked(mpExport).mockResolvedValueOnce({ count: 2, unreadable: ["A"] }).mockResolvedValueOnce(null);
    share();
    await fireEvent.click(await screen.findByLabelText(/^Multiplayer check file/));
    await tick();
    await fireEvent.click(screen.getByRole("button", { name: "Save file…" }));
    await screen.findByText(/Silo couldn't read 1 mod/);
    await fireEvent.click(screen.getByRole("button", { name: "Save file…" }));
    await waitFor(() => expect(mpExport).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.queryByText(/Saved a multiplayer check file/)).toBeNull());
    expect(screen.queryByText(/Silo couldn't read 1 mod/)).toBeNull();
  });

  it("a link names the mods it couldn't fingerprint, and a README problem is a warning (#355)", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(true));
    vi.mocked(collectionExport).mockResolvedValue({
      url: "https://silo.hllmr.com/c/?g=2",
      count: 2,
      omitted: [],
      unpinned: ["A"],
      warning: "The link works, but Silo couldn't add the page that describes the set on GitHub.",
    });
    share();
    await fireEvent.click(await screen.findByRole("button", { name: "Create link" }));
    expect(await screen.findByText("https://silo.hllmr.com/c/?g=2")).toBeInTheDocument();
    expect(screen.getByText(/Silo couldn't read 1 mod \(Title A\), so the link lists it without a fingerprint/)).toBeInTheDocument();
    expect(screen.getByText(/The link works, but Silo couldn't add the page/)).toBeInTheDocument();
  });

  it("a link names every mod it couldn't fingerprint, not just the first few", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(true));
    const many = ["M1", "M2", "M3", "M4", "M5", "M6"];
    vi.mocked(collectionExport).mockResolvedValue({ url: "https://silo.hllmr.com/c/?g=3", count: 6, omitted: [], unpinned: many, warning: null });
    share();
    await fireEvent.click(await screen.findByRole("button", { name: "Create link" }));
    expect(await screen.findByText(/Silo couldn't read 6 mods \(M1, M2, M3, M4, M5, M6\)/)).toBeInTheDocument();
  });

  it("a link still being made when the dialog closed doesn't show in the next Share (#355)", async () => {
    vi.mocked(ghStatus).mockResolvedValue(gh(true));
    let finish!: (r: unknown) => void;
    vi.mocked(collectionExport).mockReturnValueOnce(new Promise((res) => (finish = res)) as never);
    const p = share();
    await fireEvent.click(await screen.findByRole("button", { name: "Create link" }));
    await p.rerender({ open: false });
    await p.rerender({ open: true, name: "Forestry", mods: ["B"] });
    expect(await screen.findByRole("dialog", { name: "Share “Forestry”" })).toBeInTheDocument();
    finish({ url: "https://silo.hllmr.com/c/?g=old", count: 1, omitted: [], unpinned: [], warning: null });
    await tick();
    await waitFor(() => expect(screen.getByRole("button", { name: "Create link" })).toBeEnabled());
    expect(screen.queryByText("https://silo.hllmr.com/c/?g=old")).toBeNull();
    expect(p.onShared).toHaveBeenCalled();
  });
});

import MySetsTab from "./MySetsTab.svelte";
import SavegamesTab from "./SavegamesTab.svelte";
import type { Loadout } from "../../types";

describe("My sets", () => {
  const sets: Loadout[] = [
    { id: 1, name: "Harvest 2026", mods: ["A", "B"], kind: "user", createdAt: null },
    { id: 2, name: "Forestry weekend", mods: ["C", "Z"], kind: "user", createdAt: null },
    { id: 3, name: "Before Forestry weekend", mods: ["A"], kind: "auto", createdAt: new Date(new Date().getFullYear(), 9, 17, 12).getTime() },
  ];
  function mySets(extra: Record<string, unknown> = {}) {
    const props = {
      sets,
      active: new Set(["A", "B"]),
      library: new Set(["A", "B", "C"]),
      state: { kind: "set" as const, id: 1, name: "Harvest 2026" },
      onSwitch: vi.fn(),
      onUpdate: vi.fn(),
      onDelete: vi.fn(),
      onShare: vi.fn(),
      onSaveCurrent: vi.fn(),
      onOpenShared: vi.fn(),
      ...extra,
    };
    render(MySetsTab, props);
    return props;
  }

  it("compares each set with the active mods; the matching one has no Switch", async () => {
    const p = mySets();
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows.map((r) => r.querySelector("th")?.textContent?.replace(/\s+/g, " ").trim())).toEqual([
      "Harvest 2026 Active",
      "Forestry weekend",
      "Before Forestry weekend Automatic · Oct 17",
    ]);
    expect(rows[0].textContent).toContain("Same");
    expect(rows[0].querySelector("button.btn, button")?.textContent).not.toContain("Switch");
    expect(rows[1].textContent).toContain("+1 / −2");
    expect(rows[1].textContent).toContain("1 turns on and 2 are parked");
    expect(rows[1].textContent).toContain("1 not in your library");
    await fireEvent.click(screen.getByRole("button", { name: "Switch to this set" }));
    expect(p.onSwitch).toHaveBeenCalledWith(sets[1]);
    await fireEvent.click(screen.getByRole("button", { name: "Switch back" }));
    expect(p.onSwitch).toHaveBeenCalledWith(sets[2]);
  });

  it("Custom since an automatic set offers only Save as new", () => {
    mySets({ state: { kind: "custom", id: 3, name: "Before Forestry weekend", changes: 1 } });
    expect(screen.queryByRole("button", { name: "Update the set" })).toBeNull();
    expect(screen.getByRole("button", { name: "Save as new" })).toBeInTheDocument();
  });

  it("after edits offers Update the set and Save as new", async () => {
    const p = mySets({ state: { kind: "custom", id: 1, name: "Harvest 2026", changes: 3 }, active: new Set(["A"]) });
    expect(screen.getByRole("status").textContent?.replace(/\s+/g, " ")).toContain("3 changes since Harvest 2026");
    await fireEvent.click(screen.getByRole("button", { name: "Update the set" }));
    expect(p.onUpdate).toHaveBeenCalledWith(sets[0]);
    await fireEvent.click(screen.getByRole("button", { name: "Save as new" }));
    expect(p.onSaveCurrent).toHaveBeenCalled();
  });

  it("an empty list explains what a set is", () => {
    mySets({ sets: [] });
    expect(screen.getByText("No mod sets yet")).toBeInTheDocument();
  });
});

describe("Savegames", () => {
  it("shows one card per save with its mods and the missing ones on request", async () => {
    const s = {
      index: 2,
      folder: "savegame2",
      name: "Riverbend",
      mapTitle: "Riverbend Springs",
      saveDate: "2026-08-02",
      mods: [],
    };
    const st = { total: 2, present: [{ modName: "A", title: null }], missing: [{ modName: "Q", title: "Quarry" }] };
    const onFindInBrowse = vi.fn();
    const onUse = vi.fn();
    render(SavegamesTab, { rows: [{ s, st }], onUse, onBackup: vi.fn(), onSettings: vi.fn(), onFindInBrowse });
    expect(screen.getByRole("heading", { name: "Riverbend" })).toBeInTheDocument();
    expect(screen.getByText("Last played Aug 2, 2026")).toBeInTheDocument();
    expect(screen.getByText(/1 of 2 mods in your library\. 1 missing/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Show missing mods" }));
    await fireEvent.click(screen.getByRole("button", { name: "Find in Browse: Quarry" }));
    expect(onFindInBrowse).toHaveBeenCalledWith("Q", "Quarry");
    await fireEvent.click(screen.getByRole("button", { name: "Use this save's mods" }));
    expect(onUse).toHaveBeenCalledWith(s);
  });

  it("an unreadable save says why and can't be used", async () => {
    const s = {
      index: 3,
      folder: "savegame3",
      name: "savegame3",
      mapTitle: null,
      saveDate: null,
      mods: [],
      error: "careerSavegame.xml is cut off or damaged",
    };
    const st = { total: 0, present: [], missing: [] };
    const onUse = vi.fn();
    render(SavegamesTab, { rows: [{ s, st }], onUse, onBackup: vi.fn(), onSettings: vi.fn(), onFindInBrowse: vi.fn() });
    expect(screen.getByText(/couldn't read this save's mod list: careerSavegame\.xml is cut off or damaged/)).toBeInTheDocument();
    expect(screen.queryByText("Uses no mods")).toBeNull();
    const use = screen.getByRole("button", { name: "Use this save's mods" });
    expect(use).toBeDisabled();
    await fireEvent.click(use);
    expect(onUse).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Back up" })).toBeEnabled();
  });
});
