// @vitest-environment jsdom
// Open a shared set (#358): Enter in the link field is held to the same busy checks as Open, and
// a preview that was replaced can't put its conflict warnings on the one shown now.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
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
  logFrontendError: vi.fn(() => Promise.resolve()),
}));

import { collectionApply, collectionImportPreview, detectConflicts } from "../../api";
import type { Conflict, ImportPlan, ModEntry } from "../../types";
import OpenSharedTab from "./OpenSharedTab.svelte";

const mod = (techName: string): ModEntry =>
  ({ techName, title: `Title ${techName}`, version: "1.0", path: `/mods/${techName}.zip`, kind: "zip", dependencies: [] }) as unknown as ModEntry;
const library = [mod("A"), mod("B"), mod("C"), mod("D")];
const present = (t: string) => ({ techName: t, version: "1.0", source: null, installedVersion: "1.0" });

const plan = (name: string, have: string[]): ImportPlan => ({
  name,
  description: null,
  author: "farmer",
  willInstall: [{ techName: "N", version: "2.0", source: "github", installedVersion: null }],
  openPage: [],
  alreadyPresent: have.map(present),
  versionDrift: [],
  unresolved: [],
  fingerprint: `fp-${name}`,
});

const clash: Conflict = {
  id: "c1",
  severity: "critical",
  kind: "uniqueType",
  name: "seedSlot",
  explanation: "These mods use the same exclusive slot.",
  mods: ["Title A", "Title B"],
  techNames: ["A", "B"],
};

let reqN = 1000;
const req = (url: string) => ({ url, n: ++reqN });

function openTab(extra: Record<string, unknown> = {}) {
  const props = {
    library,
    active: new Set(["A"]),
    activeRefs: [],
    onSaveAsSet: vi.fn(() => Promise.resolve(1)),
    onSwitchTo: vi.fn(),
    onImported: vi.fn(),
    onTurnOn: vi.fn(),
    onPark: vi.fn(),
    onFindInBrowse: vi.fn(),
    ...extra,
  };
  const r = render(OpenSharedTab, props);
  return Object.assign(props, { rerender: r.rerender });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(detectConflicts).mockResolvedValue([]);
});

describe("Open a shared set: busy guards", () => {
  it("Enter in the link field doesn't open another set while a download runs", async () => {
    vi.mocked(collectionImportPreview).mockResolvedValue(plan("First", ["A"]));
    vi.mocked(collectionApply).mockReturnValue(new Promise(() => {}) as never);
    openTab({ request: req("https://gist.github.com/u/first") });
    await fireEvent.click(await screen.findByRole("button", { name: "Download 1 mod and save as a set" }));
    const field = screen.getByLabelText("Paste a link, or choose a file");
    await fireEvent.input(field, { target: { value: "https://gist.github.com/u/second" } });
    expect(screen.getByRole("button", { name: "Open" })).toBeDisabled();
    await fireEvent.keyDown(field, { key: "Enter" });
    await tick();
    expect(collectionImportPreview).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("heading", { name: "First" })).toBeInTheDocument();
  });

  it("a replaced preview's conflict scan doesn't land on the set shown now", async () => {
    let answerFirst!: (cs: Conflict[]) => void;
    vi.mocked(detectConflicts)
      .mockImplementationOnce(() => new Promise((r) => (answerFirst = r)))
      .mockResolvedValueOnce([]);
    vi.mocked(collectionImportPreview)
      .mockResolvedValueOnce(plan("First", ["A", "B"]))
      .mockResolvedValueOnce(plan("Second", ["C", "D"]));
    const p = openTab({ request: req("https://gist.github.com/u/first") });
    await waitFor(() => expect(detectConflicts).toHaveBeenCalledTimes(1));
    // A deep link opens the second set while the first one's conflict scan still runs.
    await p.rerender({ request: req("https://gist.github.com/u/second") });
    await screen.findByRole("heading", { name: "Second" });
    expect(detectConflicts).toHaveBeenCalledTimes(2);
    answerFirst([clash]);
    await tick();
    await tick();
    expect(screen.queryByText("Heads-up")).toBeNull();
  });
});
