// @vitest-environment jsdom
// StatusChip: text, tone and accessible label per state; click.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import StatusChip from "./StatusChip.svelte";
import { statusChipModel } from "./status";

describe("statusChipModel", () => {
  it("All good with nothing to report", () => {
    expect(statusChipModel(0, 0)).toEqual({
      tone: "ok",
      text: "All good",
      label: "Library status: All good",
    });
  });

  it("counts problems, singular and plural", () => {
    expect(statusChipModel(1, 0).text).toBe("1 problem");
    expect(statusChipModel(3, 0)).toMatchObject({ tone: "problem", text: "3 problems" });
    expect(statusChipModel(1204, 0).text).toBe("1,204 problems");
  });

  it("items to review when there are no problems", () => {
    expect(statusChipModel(0, 2)).toMatchObject({ tone: "review", text: "2 to review" });
  });

  it("problems outrank review", () => {
    expect(statusChipModel(1, 5)).toMatchObject({ tone: "problem", text: "1 problem" });
  });
});

describe("StatusChip", () => {
  it("renders the label and tone, and clicks through", async () => {
    const onclick = vi.fn();
    render(StatusChip, { problems: 1, review: 0, onclick });
    const chip = screen.getByRole("button", { name: "Library status: 1 problem" });
    expect(chip).toHaveTextContent("1 problem");
    expect(chip).toHaveAttribute("data-tone", "problem");
    await userEvent.setup().click(chip);
    expect(onclick).toHaveBeenCalledTimes(1);
  });

  it("review and ok variants", () => {
    const { unmount } = render(StatusChip, { problems: 0, review: 4 });
    expect(screen.getByRole("button", { name: "Library status: 4 to review" })).toHaveAttribute(
      "data-tone",
      "review",
    );
    unmount();
    render(StatusChip, { problems: 0, review: 0 });
    expect(screen.getByRole("button", { name: "Library status: All good" })).toHaveAttribute("data-tone", "ok");
  });
});
