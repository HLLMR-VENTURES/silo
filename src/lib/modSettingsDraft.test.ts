// The Mod settings dialog's drafts (#358): saving one file keeps the other files' unsaved edits,
// and an edit made while the write was out.
import { describe, expect, it } from "vitest";
import { afterSave, draftsOf, fieldKey, markSent, snapshotOf } from "./modSettingsDraft";
import type { SettingsFile } from "./types";

function file(path: string, values: string[], raw = `<raw ${path}/>`): SettingsFile {
  return {
    path,
    name: path,
    raw,
    fields: values.map((value, id) => ({ id, label: `f${id}`, kind: "int", value })),
  };
}

describe("mod settings drafts", () => {
  it("keys each value by file: both files' field 0 are separate", () => {
    const d = draftsOf([file("a.xml", ["1"]), file("b.xml", ["2"])]);
    expect(d.values[fieldKey("a.xml", 0)]).toBe("1");
    expect(d.values[fieldKey("b.xml", 0)]).toBe("2");
  });

  it("saving one file keeps the other file's unsaved edits and raw text", () => {
    const d = draftsOf([file("a.xml", ["1", "5"]), file("b.xml", ["2"])]);
    d.values[fieldKey("a.xml", 0)] = "10";
    d.values[fieldKey("b.xml", 0)] = "20"; // unsaved, in the other file
    d.rawText["b.xml"] = "<edited/>";
    const sent = snapshotOf(d, "a.xml", false);
    const after = afterSave(d, [file("a.xml", ["10", "5"], "<a saved/>"), file("b.xml", ["2"])], "a.xml", sent);
    expect(after.values[fieldKey("a.xml", 0)]).toBe("10");
    expect(after.values[fieldKey("b.xml", 0)]).toBe("20");
    expect(after.rawText["b.xml"]).toBe("<edited/>");
    expect(after.rawText["a.xml"]).toBe("<a saved/>");
    // b.xml's baseline is still its last reading, so its Save stays on.
    expect(after.files.find((f) => f.path === "b.xml")?.fields[0].value).toBe("2");
    expect(after.files.find((f) => f.path === "a.xml")?.fields[0].value).toBe("10");
  });

  it("an edit to the saved file made while the write was out stays a draft", () => {
    const d = draftsOf([file("a.xml", ["1", "5"])]);
    d.values[fieldKey("a.xml", 0)] = "10";
    const sent = snapshotOf(d, "a.xml", false);
    d.values[fieldKey("a.xml", 1)] = "6"; // typed during the save
    const after = afterSave(d, [file("a.xml", ["10", "5"])], "a.xml", sent);
    expect(after.values[fieldKey("a.xml", 0)]).toBe("10");
    expect(after.values[fieldKey("a.xml", 1)]).toBe("6");
    expect(after.files[0].fields[1].value).toBe("5"); // so it still reads as unsaved
  });

  it("a raw save takes the file's new fields (ids may change), and drops the old ones", () => {
    const d = draftsOf([file("a.xml", ["1", "5"])]);
    d.rawText["a.xml"] = "<one field/>";
    const sent = snapshotOf(d, "a.xml", true);
    const after = afterSave(d, [file("a.xml", ["7"], "<one field/>")], "a.xml", sent);
    expect(after.values).toEqual({ [fieldKey("a.xml", 0)]: "7" });
    expect(after.rawText["a.xml"]).toBe("<one field/>");
  });

  it("raw text typed during the save stays", () => {
    const d = draftsOf([file("a.xml", ["1"])]);
    d.rawText["a.xml"] = "<v1/>";
    const sent = snapshotOf(d, "a.xml", true);
    d.rawText["a.xml"] = "<v2/>";
    expect(afterSave(d, [file("a.xml", ["1"], "<v1/>")], "a.xml", sent).rawText["a.xml"]).toBe("<v2/>");
  });

  it("a file gone since leaves the dialog as it is", () => {
    const d = draftsOf([file("a.xml", ["1"])]);
    expect(afterSave(d, [], "a.xml", snapshotOf(d, "a.xml", false))).toBe(d);
  });

  it("a form save keeps an unsaved raw draft of the same file, and takes the new raw text if there's none", () => {
    const d = draftsOf([file("a.xml", ["1"], "<v1/>")]);
    d.rawText["a.xml"] = "<draft/>"; // typed in Raw XML, not saved
    d.values[fieldKey("a.xml", 0)] = "2";
    const after = afterSave(d, [file("a.xml", ["2"], "<v2/>")], "a.xml", snapshotOf(d, "a.xml", false));
    expect(after.rawText["a.xml"]).toBe("<draft/>");
    const clean = draftsOf([file("a.xml", ["1"], "<v1/>")]);
    clean.values[fieldKey("a.xml", 0)] = "2";
    const after2 = afterSave(clean, [file("a.xml", ["2"], "<v2/>")], "a.xml", snapshotOf(clean, "a.xml", false));
    expect(after2.rawText["a.xml"]).toBe("<v2/>");
  });

  it("a form save written but not read back: what was sent is the saved value, drafts stay", () => {
    const d = draftsOf([file("a.xml", ["1", "5"])]);
    d.values[fieldKey("a.xml", 0)] = "2";
    const sent = snapshotOf(d, "a.xml", false);
    d.values[fieldKey("a.xml", 0)] = "1"; // changed back while the write was out
    const after = markSent(d, "a.xml", sent);
    expect(after.files[0].fields.map((f) => f.value)).toEqual(["2", "5"]);
    expect(after.values[fieldKey("a.xml", 0)]).toBe("1"); // so it reads as unsaved
  });
});
