// @vitest-environment jsdom
// First run (#250): the setup flow's steps, skip, folder variants and organizing; the Organize
// dialog's auto-file choice and partial failure; What's new.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import Onboarding from "./Onboarding.svelte";
import WhatsNew from "./WhatsNew.svelte";
import OrganizePreview from "../components/OrganizePreview.svelte";
import type { ModsRootStatus } from "../api";
import type { PlannedMove } from "../types";

const status = (over: Partial<ModsRootStatus> = {}): ModsRootStatus => ({
  path: "C:\\Users\\me\\FarmingSimulator2025\\mods",
  source: "default",
  gameReads: null,
  mismatch: false,
  available: true,
  modsFolderMissing: false,
  known: [],
  canWriteGameSetting: true,
  gameSettingProblem: null,
  unresolvedOverride: null,
  layoutProblem: null,
  layoutProblemCause: null,
  layoutConflict: null,
  ...over,
});

const mv = (techName: string, category: string): PlannedMove => ({
  techName,
  fileName: `${techName}.zip`,
  category,
  relFrom: `${techName}.zip`,
  relTo: `archive/${category}/${techName}.zip`,
});
const PLAN = [mv("FS25_A", "Tractors"), mv("FS25_B", "Tractors"), mv("FS25_C", "Maps")];

function setup(over: Record<string, unknown> = {}) {
  const props = {
    modsStatus: status(),
    gameFound: true,
    scanning: false,
    scanned: true,
    progress: { done: 3, total: 3 },
    modCount: 3,
    activeCount: 3,
    archive: { path: null, isDefault: true },
    onChooseModsFolder: vi.fn(),
    onCreateModsFolder: vi.fn(),
    onUseGameFolder: vi.fn(),
    onRetry: vi.fn(),
    onPointGameHere: vi.fn(),
    onChooseArchive: vi.fn(async () => null),
    onUseDefaultArchive: vi.fn(async () => null),
    onPlan: vi.fn(async () => PLAN),
    onOrganize: vi.fn(async (_t: string[], _a: boolean, p: (d: number, t: number) => void) => {
      p(3, 3);
      return { failures: [] };
    }),
    onCopyDetails: vi.fn(),
    onfinish: vi.fn(),
    ...over,
  };
  render(Onboarding, props);
  return { props, user: userEvent.setup() };
}

describe("Onboarding", () => {
  it("walks the three steps and organizes only on the click", async () => {
    const { props, user } = setup();
    expect(screen.getByRole("heading", { name: "Silo found your mods folder" })).toBeInTheDocument();
    expect(screen.getByText("Found 3 mods.")).toBeInTheDocument();
    expect(screen.getByRole("listitem", { current: "step" })).toHaveTextContent("Your mods");
    await user.click(screen.getByRole("button", { name: "This is right" }));
    expect(screen.getByRole("heading", { name: /Active mods load in the game/ })).toHaveFocus();
    await user.click(screen.getByRole("checkbox", { name: /File new mods automatically/ }));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    // Step 3 plans (a dry run) but files nothing yet.
    expect(await screen.findByText("Ready to organize 3 mods into 2 categories")).toBeInTheDocument();
    expect(props.onPlan).toHaveBeenCalledOnce();
    expect(props.onOrganize).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "See every file" }));
    expect(screen.getByText("FS25_C.zip")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Organize 3 mods" }));
    expect(props.onOrganize).toHaveBeenCalledWith(["FS25_A", "FS25_B", "FS25_C"], true, expect.any(Function));
    expect(await screen.findByRole("heading", { name: "Your library is organized." })).toBeInTheDocument();
    expect(screen.getByText("3 mods filed, 3 active, nothing deleted.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Go to my library" }));
    expect(props.onfinish).toHaveBeenCalledWith("done");
  });

  it("waits for the scan before planning", async () => {
    const { props, user } = setup({ scanning: true, scanned: false, progress: { done: 312, total: 737 } });
    expect(screen.getByText("Reading 312 of 737 mods…")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "This is right" }));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(props.onPlan).not.toHaveBeenCalled();
  });

  it("Skip setup leaves with nothing moved", async () => {
    const { props, user } = setup();
    await user.click(screen.getByRole("button", { name: "Skip setup" }));
    expect(props.onfinish).toHaveBeenCalledWith("skip");
    expect(props.onOrganize).not.toHaveBeenCalled();
    expect(props.onPlan).not.toHaveBeenCalled();
  });

  it("Not now leaves from the last step without filing", async () => {
    const { props, user } = setup();
    await user.click(screen.getByRole("button", { name: "This is right" }));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.click(await screen.findByRole("button", { name: "Not now, show my library" }));
    expect(props.onfinish).toHaveBeenCalledWith("notNow");
    expect(props.onOrganize).not.toHaveBeenCalled();
  });

  it("lists a partial failure with Copy details", async () => {
    const { props, user } = setup({
      onOrganize: vi.fn(async () => ({
        failures: [{ techName: "FS25_C", fileName: "FS25_C.zip", message: "move: in use" }],
      })),
    });
    await user.click(screen.getByRole("button", { name: "This is right" }));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.click(await screen.findByRole("button", { name: "Organize 3 mods" }));
    expect(await screen.findByRole("heading", { name: "Silo filed 2 of 3 mods." })).toBeInTheDocument();
    expect(screen.getByText(/move: in use/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    expect(props.onCopyDetails).toHaveBeenCalledWith(expect.stringContaining("FS25_C.zip: move: in use"));
    await user.click(screen.getByRole("button", { name: "Go to my library" }));
    expect(props.onfinish).toHaveBeenCalledWith("done");
  });

  it("a refused organize stays on the summary", async () => {
    const { user } = setup({ onOrganize: vi.fn(async () => ({ refused: true })) });
    await user.click(screen.getByRole("button", { name: "This is right" }));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.click(await screen.findByRole("button", { name: "Organize 3 mods" }));
    expect(await screen.findByRole("button", { name: "Organize 3 mods" })).toBeInTheDocument();
  });

  it("a folder FS25 hasn't made yet offers Create it", async () => {
    const onCreateModsFolder = vi.fn();
    const { props, user } = setup({ modsStatus: status({ modsFolderMissing: true }), onCreateModsFolder });
    expect(screen.getByRole("heading", { name: /hasn't made its mods folder yet/ })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Create it" }));
    expect(onCreateModsFolder).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "Choose a folder…" }));
    expect(props.onChooseModsFolder).toHaveBeenCalledOnce();
  });

  it("an unavailable folder offers Try again, Choose another and the game's folder", () => {
    setup({ modsStatus: status({ source: "user", available: false, path: "E:\\Mods" }) });
    expect(screen.getByText(/Is the drive connected/)).toHaveTextContent("E:\\Mods");
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose another…" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use the game's folder" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "This is right" })).not.toBeInTheDocument();
  });

  it("explains an override, a layout problem, a cloud folder and a missing game", () => {
    setup({
      modsStatus: status({ source: "gameOverride", path: "D:\\FS25 Mods" }),
      cloudSynced: true,
      gameFound: false,
    });
    expect(screen.getByText(/Farming Simulator is set to read/)).toHaveTextContent("D:\\FS25 Mods");
    expect(screen.getByText(/synced by a cloud service/)).toBeInTheDocument();
    expect(screen.getByText(/didn't find Farming Simulator 25/)).toBeInTheDocument();
  });

  it("a layout problem shows its reason with Choose another", () => {
    setup({ modsStatus: status({ layoutProblem: "It shares files with another library." }) });
    expect(screen.getByText("It shares files with another library.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Choose another…" })).toBeInTheDocument();
  });

  it("another drive uses the picked folder, or shows why not and stays inside", async () => {
    const onChooseArchive = vi
      .fn()
      .mockResolvedValueOnce({ problem: "That folder is inside your mods folder." })
      .mockResolvedValueOnce({ path: "E:\\Silo" });
    const { user } = setup({ onChooseArchive });
    await user.click(screen.getByRole("button", { name: "This is right" }));
    await user.click(screen.getByRole("radio", { name: /On another drive/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("inside your mods folder");
    expect(screen.getByRole("radio", { name: /Inside your mods folder/ })).toBeChecked();
    await user.click(screen.getByRole("radio", { name: /On another drive/ }));
    await vi.waitFor(() => expect(onChooseArchive).toHaveBeenCalledTimes(2));
    expect(screen.getByRole("radio", { name: /On another drive/ })).toBeChecked();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("Organize dialog", () => {
  const base = {
    plan: PLAN,
    loading: false,
    applying: false,
    activeCount: 3,
    onConfirm: vi.fn(),
    onCancel: vi.fn(),
    onCopyDetails: vi.fn(),
  };

  it("shows the summary, the files and the auto-file choice", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(OrganizePreview, { ...base, onConfirm });
    expect(screen.getByRole("dialog", { name: "Organize new mods" })).toBeInTheDocument();
    expect(screen.getByText("Ready to organize 3 mods into 2 categories")).toBeInTheDocument();
    expect(screen.queryByText(/Don't preview next time/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("checkbox", { name: "File new mods automatically from now on" }));
    await user.click(screen.getByRole("button", { name: "File 3 mods" }));
    expect(onConfirm).toHaveBeenCalledWith(true);
  });

  it("files without auto-file when the box stays clear", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(OrganizePreview, { ...base, onConfirm });
    await user.click(screen.getByRole("button", { name: "File 3 mods" }));
    expect(onConfirm).toHaveBeenCalledWith(false);
  });

  it("shows progress in the footer while filing", () => {
    render(OrganizePreview, { ...base, applying: true, progress: { done: 25, total: 50 } });
    expect(screen.getByRole("progressbar", { name: "Filing 25 of 50…" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /File 3 mods/ })).not.toBeInTheDocument();
  });

  it("keeps a partial failure open with the rows marked and Copy details", async () => {
    const user = userEvent.setup();
    const onCopyDetails = vi.fn();
    const onCancel = vi.fn();
    render(OrganizePreview, {
      ...base,
      onCancel,
      onCopyDetails,
      failures: [{ techName: "FS25_B", fileName: "FS25_B.zip", message: "move: in use" }],
    });
    expect(screen.getByRole("alert")).toHaveTextContent("Silo filed 2 of 3 mods. 1 couldn't be moved:");
    expect(screen.getByText("Not moved")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    expect(onCopyDetails).toHaveBeenCalledWith(expect.stringContaining("FS25_B.zip: move: in use"));
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onCancel).toHaveBeenCalled();
  });

  it("adopting has no auto-file choice", () => {
    render(OrganizePreview, { ...base, adopt: { path: "E:\\Silo", note: null } });
    expect(screen.getByRole("dialog", { name: "File 3 mods into your library" })).toBeInTheDocument();
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
  });
});

describe("WhatsNew", () => {
  it("lists what's new and closes with Got it", async () => {
    const user = userEvent.setup();
    const onclose = vi.fn();
    render(WhatsNew, { open: true, onclose });
    const dialog = screen.getByRole("dialog", { name: "What's new in Silo 1.0" });
    for (const t of ["A new layout", "Mod sets", "Undo", "Keyboard shortcuts", "Plain-English errors"])
      expect(dialog).toHaveTextContent(t);
    await user.click(screen.getByRole("button", { name: "Got it" }));
    expect(onclose).toHaveBeenCalledOnce();
  });
});
