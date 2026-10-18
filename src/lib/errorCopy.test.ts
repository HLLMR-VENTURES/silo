import { afterEach, describe, expect, it, vi } from "vitest";
import { AppError } from "./errors";
import {
  classifyMessage,
  describeError,
  ERROR_CODES,
  fileNameIn,
  formatDetail,
  rememberAppVersion,
} from "./errorCopy";

afterEach(() => {
  vi.unstubAllGlobals();
  rememberAppVersion(null);
});

const WINDOWS_UA = { userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)", platform: "Win32" };
const LINUX_UA = { userAgent: "Mozilla/5.0 (X11; Linux x86_64)", platform: "Linux x86_64" };

describe("describeError copy", () => {
  const table: [code: string, title: string, retryable: boolean][] = [
    ["FILE_IN_USE", "Silo couldn't change a file because another program is using it. Close Farming Simulator 25 or the program that has it open, then try again.", true],
    ["GAME_RUNNING", "Close Farming Simulator 25 first, then try again.", true],
    ["LIBRARY_BUSY", "Silo is finishing another change to your library. Try again in a moment.", true],
    ["ARCHIVE_UNAVAILABLE", "Your library archive isn't available. Connect the drive, or choose another folder.", true],
    ["PERMISSION_DENIED", "Windows didn't let Silo change this file. If it's in a protected folder, choose a mods folder in your Documents.", false],
    ["DISK_FULL", "There isn't enough space on the drive for this. Free some space and try again.", true],
    ["NOT_FOUND", "Silo couldn't find a file it needed. It may have been moved or deleted. Rescan and try again.", false],
    ["NET_OFFLINE", "Silo can't reach the internet right now.", true],
    ["NET_TIMEOUT", "The download stopped responding.", true],
    ["GITHUB_AUTH", "GitHub needs you to sign in again.", false],
    ["GITHUB_RATE_LIMIT", "GitHub is limiting requests for a while. Connecting your GitHub account raises the limit.", false],
    ["NOT_A_MOD", "That file isn't a Farming Simulator mod.", false],
    ["CHANGED_SINCE_PREVIEW", "This changed after you opened it. Preview it again.", false],
    ["DB_NEWER", "This library was used by a newer Silo. Install the latest Silo to open it.", false],
    ["UNKNOWN", "Something went wrong while moving mods.", true],
  ];

  it.each(table)("%s", (code, title, retryable) => {
    vi.stubGlobal("navigator", WINDOWS_UA);
    const d = describeError({ code, message: "raw text" }, "Moving mods");
    expect(d).toMatchObject({ code, title, retryable, silent: false });
    expect(d.detail).toMatchObject({ code, message: "raw text", op: "Moving mods" });
    expect(Number.isNaN(Date.parse(d.detail.at))).toBe(false);
  });

  it("covers every code", () => {
    const covered = new Set([...table.map((r) => r[0]), "STILL_LOADING"]);
    expect([...ERROR_CODES].filter((c) => !covered.has(c))).toEqual([]);
  });

  it("copy is plain: no exclamation marks or em-dashes", () => {
    for (const code of ERROR_CODES) {
      const t = describeError(new AppError(code, "x"), "Doing it").title;
      expect(t).not.toMatch(/[!\u2014]/);
    }
  });

  it("STILL_LOADING is silent with no title", () => {
    const d = describeError(new AppError("STILL_LOADING", "Silo is still loading"));
    expect(d).toMatchObject({ code: "STILL_LOADING", title: "", silent: true, retryable: false });
  });

  it("OTHER and codes this build doesn't know read as UNKNOWN, keeping the raw code in detail", () => {
    const other = describeError({ code: "OTHER", message: "x" }, "Saving");
    expect(other.code).toBe("UNKNOWN");
    expect(other.title).toBe("Something went wrong while saving.");
    expect(other.detail.code).toBe("OTHER");
    expect(describeError({ code: "BRAND_NEW", message: "x" }).code).toBe("UNKNOWN");
  });

  it("interpolates the op, lowercased first, with a default", () => {
    expect(describeError("boom", "Checking for updates").title).toBe(
      "Something went wrong while checking for updates.",
    );
    expect(describeError("boom").title).toBe("Something went wrong while doing that.");
    expect(describeError("boom").detail.op).toBe("");
  });

  it("names the file when a FILE_IN_USE message mentions one", () => {
    const msg =
      'failed to rename "C:\\Users\\me\\Documents\\My Games\\FarmingSimulator2025\\mods\\FS25_Big_Tractor.zip": The process cannot access the file because it is being used by another process. (os error 32)';
    expect(describeError(msg).title).toBe(
      "Silo couldn't move FS25_Big_Tractor.zip because another program is using it. Close Farming Simulator 25 or the program that has it open, then try again.",
    );
  });

  it("says 'Your system' for PERMISSION_DENIED off Windows", () => {
    vi.stubGlobal("navigator", LINUX_UA);
    expect(describeError(new AppError("PERMISSION_DENIED", "x")).title).toMatch(
      /^Your system didn't let Silo change this file\./,
    );
    vi.stubGlobal("navigator", { userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) darwin", platform: "MacIntel" });
    expect(describeError(new AppError("PERMISSION_DENIED", "x")).title).toMatch(/^Your system/);
  });
});

describe("classifyMessage", () => {
  const cases: [msg: string, code: string][] = [
    ["The process cannot access the file because it is being used by another process. (os error 32)", "FILE_IN_USE"],
    ["The process cannot access the file because another process has locked a portion of the file. (os error 33)", "FILE_IN_USE"],
    ["Device or resource busy (os error 16)", "FILE_IN_USE"],
    ["Text file busy (os error 26)", "FILE_IN_USE"],
    ["Access is denied. (os error 5)", "PERMISSION_DENIED"],
    ["Permission denied (os error 13)", "PERMISSION_DENIED"],
    ["Operation not permitted (os error 1)", "PERMISSION_DENIED"],
    ["There is not enough space on the disk. (os error 112)", "DISK_FULL"],
    ["The disk is full. (os error 39)", "DISK_FULL"],
    ["No space left on device (os error 28)", "DISK_FULL"],
    ["The system cannot find the file specified. (os error 2)", "NOT_FOUND"],
    ["No such file or directory (os error 2)", "NOT_FOUND"],
    ["The system cannot find the path specified. (os error 3)", "NOT_FOUND"],
    ["A connection attempt failed because the connected party did not properly respond (os error 10060)", "NET_TIMEOUT"],
    ["error sending request: operation timed out", "NET_TIMEOUT"],
    ["request timeout after 30s", "NET_TIMEOUT"],
    ["No connection could be made because the target machine actively refused it. (os error 10061)", "NET_OFFLINE"],
    ["A socket operation was attempted to an unreachable network. (os error 10051)", "NET_OFFLINE"],
    ["No such host is known. (os error 11001)", "NET_OFFLINE"],
    ["error trying to connect: dns error: failed to lookup address information", "NET_OFFLINE"],
    ["tcp connect error: Connection refused (os error 111)", "NET_OFFLINE"],
    ["Network is unreachable (os error 101)", "NET_OFFLINE"],
    ["GitHub said: Bad credentials", "GITHUB_AUTH"],
    ["GitHub API returned 401 Unauthorized", "GITHUB_AUTH"],
    ["API rate limit exceeded for 1.2.3.4", "GITHUB_RATE_LIMIT"],
    ["something odd happened", "UNKNOWN"],
    ["HTTP 401 from example.com", "UNKNOWN"],
  ];
  it.each(cases)("%s -> %s", (msg, code) => expect(classifyMessage(msg)).toBe(code));

  it("matches whole OS error numbers only (320 is not 32, 12 is not 1 or 2)", () => {
    expect(classifyMessage("weird (os error 320)")).toBe("UNKNOWN");
    expect(classifyMessage("weird (os error 12)")).toBe("UNKNOWN");
    expect(classifyMessage("weird (os error 1120)")).toBe("UNKNOWN");
  });

  it("a code-less error uses the classification", () => {
    vi.stubGlobal("navigator", WINDOWS_UA);
    const d = describeError(new AppError(null, "Access is denied. (os error 5)"));
    expect(d.code).toBe("PERMISSION_DENIED");
    expect(d.detail.code).toBe("PERMISSION_DENIED");
  });
});

describe("fileNameIn", () => {
  it("takes the base name of a path", () => {
    expect(fileNameIn('rename "/home/me/mods/FS25_A.zip" failed')).toBe("FS25_A.zip");
    expect(fileNameIn("C:\\mods\\FS25_B.zip: in use")).toBe("FS25_B.zip");
    expect(fileNameIn("no file here")).toBeNull();
  });
});

describe("formatDetail", () => {
  it("lists version, time, op, code and message", () => {
    const d = { code: "FILE_IN_USE", message: "in use (os error 32)", op: "Moving mods", at: "2026-10-17T12:00:00.000Z" };
    expect(formatDetail(d, "0.11.0")).toBe(
      "Silo 0.11.0\nTime: 2026-10-17T12:00:00.000Z\nWhile: Moving mods\nCode: FILE_IN_USE\nMessage: in use (os error 32)",
    );
  });
  it("uses the remembered version, and copes without one", () => {
    const d = { code: "UNKNOWN", message: "x", op: "", at: "t" };
    expect(formatDetail(d)).toBe("Silo (version unknown)\nTime: t\nCode: UNKNOWN\nMessage: x");
    rememberAppVersion("1.2.3");
    expect(formatDetail(d).split("\n")[0]).toBe("Silo 1.2.3");
  });
});
