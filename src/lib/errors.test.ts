import { describe, expect, it, vi } from "vitest";
import { AppError, errorCode, toAppError } from "./errors";

// The wrapper test below drives api.ts with a fake Tauri `invoke`; everything else in the
// module stays real (plugin-dialog / event import more than `invoke` from it).
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(),
}));
import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { archiveStatus, serial } from "./api";

describe("toAppError", () => {
  it("keeps the code and message of a typed backend error", () => {
    const e = toAppError({ code: "STILL_LOADING", message: "Silo is still loading" });
    expect(e.code).toBe("STILL_LOADING");
    expect(String(e)).toBe("Silo is still loading");
    expect(`${e}`).toBe("Silo is still loading");
  });
  it("wraps a plain string error without a code", () => {
    const e = toAppError("boom");
    expect(e.code).toBeNull();
    expect(String(e)).toBe("boom");
  });
  it("leaves an AppError as it is", () => {
    const e = new AppError("LIBRARY_BUSY", "busy");
    expect(toAppError(e)).toBe(e);
    expect(errorCode(e)).toBe("LIBRARY_BUSY");
  });
  it("is named AppError but still reads as just its message", () => {
    const e = new AppError("LIBRARY_BUSY", "busy");
    expect(e.name).toBe("AppError");
    expect(e).toBeInstanceOf(Error);
    expect(String(e)).toBe("busy");
  });
});

describe("the invoke wrapper", () => {
  const busy = "Silo is busy with another change to your library. Try again in a moment.";

  it("rejects a plain string error as an AppError that still reads as that string", async () => {
    vi.mocked(rawInvoke).mockRejectedValueOnce(busy);
    const e: unknown = await archiveStatus().catch((x: unknown) => x);
    expect(e).toBeInstanceOf(AppError);
    expect(errorCode(e)).toBeNull();
    // Every UI site shows errors with String(e) or a template literal.
    expect(String(e)).toBe(busy);
    expect(`${e}`).toBe(busy);
    expect(`Couldn't organize: ${e}`).toBe(`Couldn't organize: ${busy}`);
  });

  it("rejects a typed error with its code, reading as its message", async () => {
    vi.mocked(rawInvoke).mockRejectedValueOnce({
      code: "STILL_LOADING",
      message: "Silo is still loading your library — try again in a moment.",
    });
    const e: unknown = await archiveStatus().catch((x: unknown) => x);
    expect(errorCode(e)).toBe("STILL_LOADING");
    expect(String(e)).toBe("Silo is still loading your library — try again in a moment.");
  });

  it("passes results through untouched", async () => {
    vi.mocked(rawInvoke).mockResolvedValueOnce({ path: null });
    await expect(archiveStatus()).resolves.toEqual({ path: null });
  });

  it("keeps the serial queue going after a rejected write", async () => {
    vi.mocked(rawInvoke).mockRejectedValueOnce("first failed").mockResolvedValueOnce("ok");
    const first = serial("errors-test", () => archiveStatus()).catch((x: unknown) => String(x));
    const second = serial("errors-test", () => archiveStatus());
    expect(await first).toBe("first failed");
    expect(await second).toBe("ok");
  });
});
