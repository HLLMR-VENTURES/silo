// @vitest-environment jsdom
// The fill type bridge tool: what it sends to the generator.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";

const api = vi.hoisted(() => ({ generateBridge: vi.fn(() => Promise.resolve("FS25_SiloFilltypeBridge.zip")) }));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  generateBridge: api.generateBridge,
}));

import BridgeTool from "./BridgeTool.svelte";

describe("BridgeTool", () => {
  it("a fill type typed twice (any case) is one chip and is sent once", async () => {
    const user = userEvent.setup();
    render(BridgeTool, { onGenerated: vi.fn(), onClose: vi.fn() });
    await user.type(screen.getByLabelText("Fill type name(s)"), "sugarbeet SUGARBEET, beetpulp");
    expect(screen.getAllByText("SUGARBEET")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: /Generate/ }));
    expect(api.generateBridge).toHaveBeenCalledWith(expect.objectContaining({ fillTypes: ["SUGARBEET", "BEETPULP"] }));
  });
});
