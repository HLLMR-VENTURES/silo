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
}));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getConfig: api.getConfig,
  setConfig: api.setConfig,
  getModSettings: api.getModSettings,
  saveModSettings: api.saveModSettings,
}));

import ConfigEditor from "./ConfigEditor.svelte";
import ModSettings from "./ModSettings.svelte";
import { isNumberText, type CfgField } from "../configSchemas";

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
});

describe("isNumberText", () => {
  it("takes plain numbers only", () => {
    for (const ok of ["6", "-1", "0.75", ".5", "10."]) expect(isNumberText(ok)).toBe(true);
    for (const bad of ["", " ", "1e3", "abc", "1.2.3"]) expect(isNumberText(bad)).toBe(false);
    expect(isNumberText("2", true)).toBe(true);
    expect(isNumberText("2.5", true)).toBe(false);
  });
});
