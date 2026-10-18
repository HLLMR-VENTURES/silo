// @vitest-environment jsdom
// Keymap resolution: each shortcut, typing contexts, modifiers, IME composition, the modal gate,
// and the reload keys that are always swallowed.
import { describe, expect, it, vi } from "vitest";
import {
  installShortcuts,
  isTypingTarget,
  resolveShortcut,
  SHORTCUT_DOCS,
  VIEW_SHORTCUTS,
  viewKeyshortcut,
  type KeyInput,
  type ShortcutContext,
} from "./shortcuts";
import { NAV_ITEMS } from "../components/shell/navItems";

const idle: ShortcutContext = { typing: false, modalOpen: false };
const typing: ShortcutContext = { typing: true, modalOpen: false };
const modal: ShortcutContext = { typing: false, modalOpen: true };

const act = (e: KeyInput, ctx: ShortcutContext = idle) => resolveShortcut(e, ctx).action;

describe("resolveShortcut", () => {
  it("maps each shortcut to its action", () => {
    expect(act({ key: "k", ctrlKey: true })).toEqual({ kind: "focusSearch" });
    expect(act({ key: "/" })).toEqual({ kind: "focusSearch" });
    expect(act({ key: "F5" })).toEqual({ kind: "rescan" });
    expect(act({ key: "r", ctrlKey: true })).toEqual({ kind: "rescan" });
    expect(act({ key: "R", ctrlKey: true, shiftKey: true })).toEqual({ kind: "rescan" });
    expect(act({ key: ",", ctrlKey: true })).toEqual({ kind: "settings" });
    expect(act({ key: "1", code: "Digit1", altKey: true })).toEqual({ kind: "view", view: "library" });
    expect(act({ key: "5", code: "Digit5", altKey: true })).toEqual({ kind: "view", view: "updates" });
    expect(act({ key: "ArrowLeft", altKey: true })).toEqual({ kind: "back" });
    expect(act({ key: "F6" })).toEqual({ kind: "region", dir: 1 });
    expect(act({ key: "F6", shiftKey: true })).toEqual({ kind: "region", dir: -1 });
    expect(act({ key: "?", shiftKey: true })).toEqual({ kind: "help" });
  });

  it("swallows the keys it acts on", () => {
    expect(resolveShortcut({ key: "k", ctrlKey: true }, idle).preventDefault).toBe(true);
    expect(resolveShortcut({ key: "?", shiftKey: true }, idle).preventDefault).toBe(true);
    expect(resolveShortcut({ key: "a" }, idle)).toEqual({ action: null, preventDefault: false });
  });

  it("finds Alt+digit by code when the layout types a symbol", () => {
    expect(act({ key: "&", code: "Digit1", altKey: true })).toEqual({ kind: "view", view: "library" });
    expect(act({ key: "6", code: "Digit6", altKey: true })).toBeNull();
  });

  it("never takes bare letters, digits or Alt+digit with other modifiers", () => {
    for (const key of ["1", "5", "k", "r", "a", " "]) expect(act({ key })).toBeNull();
    expect(act({ key: "1", code: "Digit1", altKey: true, ctrlKey: true })).toBeNull(); // AltGr
    expect(act({ key: "1", code: "Digit1", altKey: true, shiftKey: true })).toBeNull();
    expect(act({ key: "ArrowLeft" })).toBeNull();
    expect(act({ key: "/", shiftKey: true })).toBeNull();
    expect(act({ key: "k", ctrlKey: true, altKey: true })).toBeNull();
    expect(act({ key: "k", metaKey: true })).toBeNull();
  });

  it("while typing, keeps only Ctrl+K, Ctrl+, F6 and the reload keys", () => {
    expect(act({ key: "k", ctrlKey: true }, typing)).toEqual({ kind: "focusSearch" });
    expect(act({ key: ",", ctrlKey: true }, typing)).toEqual({ kind: "settings" });
    expect(act({ key: "F6" }, typing)).toEqual({ kind: "region", dir: 1 });
    expect(act({ key: "F5" }, typing)).toEqual({ kind: "rescan" });
    expect(act({ key: "r", ctrlKey: true }, typing)).toEqual({ kind: "rescan" });
    expect(resolveShortcut({ key: "/" }, typing)).toEqual({ action: null, preventDefault: false });
    expect(resolveShortcut({ key: "?", shiftKey: true }, typing).action).toBeNull();
    expect(act({ key: "1", code: "Digit1", altKey: true }, typing)).toBeNull();
    expect(act({ key: "ArrowLeft", altKey: true }, typing)).toBeNull();
  });

  it("ignores everything while an IME is composing", () => {
    for (const key of ["F5", "F6", "/", "?"]) {
      expect(resolveShortcut({ key, isComposing: true }, idle)).toEqual({ action: null, preventDefault: false });
    }
    expect(resolveShortcut({ key: "k", ctrlKey: true, isComposing: true }, idle).action).toBeNull();
  });

  it("does nothing behind a modal dialog, but still never lets a reload key through", () => {
    for (const e of [
      { key: "k", ctrlKey: true },
      { key: "F6" },
      { key: "?" },
      { key: "1", code: "Digit1", altKey: true },
      { key: ",", ctrlKey: true },
    ]) {
      expect(resolveShortcut(e, modal)).toEqual({ action: null, preventDefault: false });
    }
    expect(resolveShortcut({ key: "F5" }, modal)).toEqual({ action: null, preventDefault: true });
    expect(resolveShortcut({ key: "r", ctrlKey: true }, modal)).toEqual({ action: null, preventDefault: true });
  });

  it("swallows a held reload key without rescanning again", () => {
    expect(resolveShortcut({ key: "F5", repeat: true }, idle)).toEqual({ action: null, preventDefault: true });
    expect(act({ key: "F5", ctrlKey: true })).toEqual({ kind: "rescan" });
    expect(act({ key: "F5", altKey: true })).toBeNull();
  });

  it("leaves a key alone that something nearer already handled", () => {
    expect(act({ key: "?", defaultPrevented: true })).toBeNull();
    expect(act({ key: "F6", defaultPrevented: true })).toBeNull();
    expect(resolveShortcut({ key: "F5", defaultPrevented: true }, idle).preventDefault).toBe(true);
  });
});

describe("isTypingTarget", () => {
  it("is true for text fields, textareas, selects and contenteditable", () => {
    document.body.innerHTML = `
      <input id="t" /><input id="s" type="search" /><input id="c" type="checkbox" />
      <input id="r" type="range" /><textarea id="ta"></textarea><select id="sel"></select>
      <div contenteditable="true"><span id="ce">x</span></div>
      <div contenteditable="false"><span id="nce">x</span></div>
      <button id="b">b</button>`;
    const el = (id: string) => document.getElementById(id);
    expect(isTypingTarget(el("t"))).toBe(true);
    expect(isTypingTarget(el("s"))).toBe(true);
    expect(isTypingTarget(el("ta"))).toBe(true);
    expect(isTypingTarget(el("sel"))).toBe(true);
    expect(isTypingTarget(el("ce"))).toBe(true);
    expect(isTypingTarget(el("c"))).toBe(false);
    expect(isTypingTarget(el("r"))).toBe(false);
    expect(isTypingTarget(el("nce"))).toBe(false);
    expect(isTypingTarget(el("b"))).toBe(false);
    expect(isTypingTarget(document.body)).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
    expect(isTypingTarget(window)).toBe(false);
    document.body.innerHTML = "";
  });
});

describe("shortcut metadata", () => {
  it("Alt+1…Alt+5 follow the rail's main group", () => {
    expect(VIEW_SHORTCUTS).toEqual(NAV_ITEMS.filter((i) => i.group === "main").map((i) => i.id));
    expect(viewKeyshortcut("library")).toBe("Alt+1");
    expect(viewKeyshortcut("updates")).toBe("Alt+5");
    expect(viewKeyshortcut("settings")).toBe("Control+,");
    expect(viewKeyshortcut("tools")).toBeUndefined();
  });

  it("documents every shortcut once", () => {
    const labels = SHORTCUT_DOCS.map((d) => d.label);
    expect(new Set(labels).size).toBe(labels.length);
    expect(labels).toContain("Rescan your library");
    expect(labels).toContain("Show keyboard shortcuts");
  });
});

describe("installShortcuts", () => {
  it("runs resolved actions, prevents the default and removes its listener", () => {
    const run = vi.fn();
    const stop = installShortcuts(window, run, () => false);
    const f5 = new KeyboardEvent("keydown", { key: "F5", cancelable: true });
    window.dispatchEvent(f5);
    expect(f5.defaultPrevented).toBe(true);
    expect(run).toHaveBeenCalledWith({ kind: "rescan" });
    const a = new KeyboardEvent("keydown", { key: "a", cancelable: true });
    window.dispatchEvent(a);
    expect(a.defaultPrevented).toBe(false);
    expect(run).toHaveBeenCalledTimes(1);
    stop();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "F5", cancelable: true }));
    expect(run).toHaveBeenCalledTimes(1);
  });

  it("asks whether a modal is open on every key", () => {
    const run = vi.fn();
    let open = true;
    const stop = installShortcuts(window, run, () => open);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "?" }));
    expect(run).not.toHaveBeenCalled();
    open = false;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "?" }));
    expect(run).toHaveBeenCalledWith({ kind: "help" });
    stop();
  });
});
