// @vitest-environment jsdom
// The settings-file editors (game.xml, a savegame, a mod's settings): what Save sends to Rust.
// Rust's edit values are Strings, and a number input bound with bind:value holds a JS number.
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";

const api = vi.hoisted(() => ({
  getConfig: vi.fn(),
  setConfig: vi.fn(),
  getModSettings: vi.fn(),
  saveModSettings: vi.fn(),
  saveModSettingsRaw: vi.fn(),
}));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getConfig: api.getConfig,
  setConfig: api.setConfig,
  getModSettings: api.getModSettings,
  saveModSettings: api.saveModSettings,
  saveModSettingsRaw: api.saveModSettingsRaw,
}));

import ConfigEditor from "./ConfigEditor.svelte";
import ModSettings from "./ModSettings.svelte";
import { inFileCase, isNumberText, matchOptionCase, type CfgField } from "../configSchemas";

const FIELDS: CfgField[] = [
  { path: "a.n", label: "Max lights", kind: "number" },
  { path: "a.s", label: "Quality", kind: "select", options: ["low", "high"] },
];

function open() {
  render(ConfigEditor, { title: "Configure: game.xml", path: "C:/game.xml", fields: FIELDS, onClose: vi.fn() });
}

describe("ConfigEditor", () => {
  beforeEach(() => {
    api.getConfig.mockReset().mockResolvedValue({ "a.n": "4", "a.s": "low" });
    api.setConfig.mockReset().mockResolvedValue(undefined);
  });

  it("sends a typed number as a string, as Rust expects", async () => {
    open();
    const n = await screen.findByLabelText(/Max lights/);
    await fireEvent.input(n, { target: { value: "6" } });
    await userEvent.setup().click(screen.getByRole("button", { name: "Save changes" }));
    expect(api.setConfig).toHaveBeenCalledWith("C:/game.xml", [{ path: "a.n", value: "6" }]);
  });

  it("doesn't save an emptied number field", async () => {
    open();
    const n = await screen.findByLabelText(/Max lights/);
    await fireEvent.input(n, { target: { value: "" } });
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
  });

  it("a failed save keeps the form and the edits, says why, and can be tried again", async () => {
    api.setConfig.mockRejectedValueOnce({ code: "CONFIG_GAME_RUNNING", message: "Close Farming Simulator 25 first." });
    open();
    const n = await screen.findByLabelText(/Max lights/);
    await fireEvent.input(n, { target: { value: "6" } });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByLabelText(/Max lights/)).toHaveValue(6);
    const save = screen.getByRole("button", { name: "Save changes" });
    expect(save).toBeEnabled();
    await user.click(save);
    await waitFor(() => expect(api.setConfig).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("is off while a save is out, and an edit made meanwhile stays unsaved (#358)", async () => {
    let finish!: () => void;
    api.setConfig.mockImplementationOnce(() => new Promise<void>((r) => (finish = r)));
    open();
    const n = await screen.findByLabelText(/Max lights/);
    await fireEvent.input(n, { target: { value: "6" } });
    const user = userEvent.setup();
    const save = screen.getByRole("button", { name: "Save changes" });
    await user.click(save);
    expect(save).toBeDisabled();
    await user.click(save); // a double-click: still one write
    await fireEvent.input(n, { target: { value: "7" } });
    finish();
    await waitFor(() => expect(save).toBeEnabled());
    expect(api.setConfig).toHaveBeenCalledTimes(1);
    expect(api.setConfig).toHaveBeenCalledWith("C:/game.xml", [{ path: "a.n", value: "6" }]);
    await user.click(save);
    await waitFor(() => expect(api.setConfig).toHaveBeenCalledTimes(2));
    expect(api.setConfig).toHaveBeenLastCalledWith("C:/game.xml", [{ path: "a.n", value: "7" }]);
  });
});

describe("ConfigEditor: the game's casing", () => {
  beforeEach(() => {
    api.getConfig.mockReset().mockResolvedValue({ "a.n": "4", "a.s": "High" });
    api.setConfig.mockReset().mockResolvedValue(undefined);
  });

  it("shows 'High' as the high option, and doesn't write it back unchanged", async () => {
    open();
    const sel = (await screen.findByLabelText(/Quality/)) as HTMLSelectElement;
    expect(sel.value).toBe("high");
    expect(screen.getByRole("button", { name: "Save changes" })).toBeDisabled();
    expect(matchOptionCase(FIELDS, { "a.s": "Medium" })).toEqual({ "a.s": "Medium" });
    expect(matchOptionCase(FIELDS, { "a.s": "LOW" })).toEqual({ "a.s": "low" });
  });
});

describe("ConfigEditor: writing a choice", () => {
  it("writes the choice in the file's casing ('High' in the file → 'Low')", async () => {
    api.getConfig.mockReset().mockResolvedValue({ "a.n": "4", "a.s": "High" });
    api.setConfig.mockReset().mockResolvedValue(undefined);
    open();
    const sel = await screen.findByLabelText(/Quality/);
    await fireEvent.change(sel, { target: { value: "low" } });
    await userEvent.setup().click(screen.getByRole("button", { name: "Save changes" }));
    expect(api.setConfig).toHaveBeenCalledWith("C:/game.xml", [{ path: "a.s", value: "Low" }]);
  });

  it("keeps Silo's spelling when the file writes it lowercase, or has nothing", () => {
    expect(inFileCase("veryHigh", "Medium")).toBe("VeryHigh");
    expect(inFileCase("low", "auto")).toBe("low");
    expect(inFileCase("low", undefined)).toBe("low");
    expect(inFileCase("8", "4")).toBe("8");
  });
});

describe("ModSettings", () => {
  beforeEach(() => {
    api.getModSettings.mockReset().mockResolvedValue([
      {
        path: "C:/modSettings/FS25_X/settings.xml",
        name: "settings.xml",
        raw: "<s/>",
        fields: [
          { id: 1, label: "Workers", kind: "int", value: "3" },
          { id: 2, label: "Speed", kind: "float", value: "1.5" },
        ],
      },
    ]);
    api.saveModSettings.mockReset().mockResolvedValue(undefined);
  });

  it("sends typed numbers as strings, and refuses a fraction in a whole-number field", async () => {
    render(ModSettings, { modName: "FS25_X", title: "X", onClose: vi.fn() });
    const workers = await screen.findByLabelText("Workers");
    const save = screen.getByRole("button", { name: "Save changes" });
    await fireEvent.input(workers, { target: { value: "2.5" } });
    expect(save).toBeDisabled();
    await fireEvent.input(workers, { target: { value: "5" } });
    await fireEvent.input(screen.getByLabelText("Speed"), { target: { value: "2.25" } });
    await userEvent.setup().click(save);
    expect(api.saveModSettings).toHaveBeenCalledWith("C:/modSettings/FS25_X/settings.xml", [
      { id: 1, value: "5" },
      { id: 2, value: "2.25" },
    ]);
  });

  it("saving one file keeps the unsaved edits in the mod's other file (#358)", async () => {
    const two = (a: string) => [
      { path: "C:/ms/a.xml", name: "a.xml", raw: "<a/>", fields: [{ id: 0, label: "Alpha", kind: "int", value: a }] },
      { path: "C:/ms/b.xml", name: "b.xml", raw: "<b/>", fields: [{ id: 0, label: "Beta", kind: "int", value: "2" }] },
    ];
    api.getModSettings.mockReset().mockResolvedValueOnce(two("1")).mockResolvedValue(two("10"));
    render(ModSettings, { modName: "FS25_X", title: "X", onClose: vi.fn() });
    const alpha = await screen.findByLabelText("Alpha");
    const beta = screen.getByLabelText("Beta");
    // Both files have a field 0: editing one doesn't touch the other.
    await fireEvent.input(alpha, { target: { value: "10" } });
    expect(beta).toHaveValue(2);
    await fireEvent.input(beta, { target: { value: "20" } });
    const [saveA, saveB] = screen.getAllByRole("button", { name: "Save changes" });
    await userEvent.setup().click(saveA);
    expect(api.saveModSettings).toHaveBeenCalledWith("C:/ms/a.xml", [{ id: 0, value: "10" }]);
    await waitFor(() => expect(api.getModSettings).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(saveA).toBeDisabled()); // saved: nothing left to save there
    expect(screen.getByLabelText("Beta")).toHaveValue(20);
    expect(saveB).toBeEnabled();
  });

  it("keeps a file's Save and Raw XML off until the file is read back after its save (#358)", async () => {
    const one = (v: string) => [
      { path: "C:/ms/a.xml", name: "a.xml", raw: "<a/>", fields: [{ id: 0, label: "Alpha", kind: "int", value: v }] },
    ];
    let readBack!: (f: ReturnType<typeof one>) => void;
    api.getModSettings
      .mockReset()
      .mockResolvedValueOnce(one("1"))
      .mockImplementationOnce(() => new Promise((r) => (readBack = r)));
    render(ModSettings, { modName: "FS25_X", title: "X", onClose: vi.fn() });
    await fireEvent.input(await screen.findByLabelText("Alpha"), { target: { value: "5" } });
    const save = screen.getByRole("button", { name: "Save changes" });
    const user = userEvent.setup();
    await user.click(save);
    await waitFor(() => expect(api.getModSettings).toHaveBeenCalledTimes(2));
    expect(save).toBeDisabled();
    expect(screen.getByRole("button", { name: "Raw XML" })).toBeDisabled();
    await user.click(save);
    expect(api.saveModSettings).toHaveBeenCalledTimes(1);
    readBack(one("5"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Raw XML" })).toBeEnabled());
    expect(save).toBeDisabled(); // saved: nothing left to save
  });
});

describe("ModSettings: raw XML saves (#358)", () => {
  const one = (raw: string, labels: string[]) => [
    {
      path: "C:/ms/a.xml",
      name: "a.xml",
      raw,
      fields: labels.map((label, id) => ({ id, label, kind: "int", value: "1" })),
    },
  ];
  beforeEach(() => {
    api.saveModSettingsRaw.mockReset().mockResolvedValue(undefined);
  });

  it("stays on Raw XML when the file can't be read back: the form's fields may have moved", async () => {
    api.getModSettings.mockReset().mockResolvedValueOnce(one("<a/>", ["Alpha"])).mockRejectedValueOnce("gone");
    render(ModSettings, { modName: "FS25_X", title: "X", onClose: vi.fn() });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Raw XML" }));
    await fireEvent.input(screen.getByLabelText("Raw XML of a.xml"), { target: { value: "<x/><a/>" } });
    await user.click(screen.getByRole("button", { name: "Save raw XML" }));
    await waitFor(() => expect(api.getModSettings).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByRole("button", { name: "Save raw XML" })).toBeEnabled());
    expect(screen.getByLabelText("Raw XML of a.xml")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Raw XML" })).toBeDisabled();
    expect(screen.queryByLabelText("Alpha")).toBeNull();
  });

  it("stays on Raw XML, with the text, when it was edited while the save ran", async () => {
    let finish!: () => void;
    api.saveModSettingsRaw.mockImplementationOnce(() => new Promise<void>((r) => (finish = r)));
    api.getModSettings.mockReset().mockResolvedValueOnce(one("<a/>", ["Alpha"])).mockResolvedValue(one("<v1/>", ["Alpha"]));
    render(ModSettings, { modName: "FS25_X", title: "X", onClose: vi.fn() });
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Raw XML" }));
    const area = screen.getByLabelText("Raw XML of a.xml");
    await fireEvent.input(area, { target: { value: "<v1/>" } });
    await user.click(screen.getByRole("button", { name: "Save raw XML" }));
    await fireEvent.input(area, { target: { value: "<v2/>" } });
    finish();
    await waitFor(() => expect(api.getModSettings).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByRole("button", { name: "Raw XML" })).toBeEnabled());
    expect((screen.getByLabelText("Raw XML of a.xml") as HTMLTextAreaElement).value).toBe("<v2/>");
  });
});

describe("isNumberText", () => {
  it("takes plain numbers only", () => {
    for (const ok of ["6", "-1", "0.75", ".5", "10."]) expect(isNumberText(ok)).toBe(true);
    for (const bad of ["", " ", "1e3", "abc", "1.2.3"]) expect(isNumberText(bad)).toBe(false);
    expect(isNumberText("2", true)).toBe(true);
    expect(isNumberText("2.5", true)).toBe(false);
  });
});
