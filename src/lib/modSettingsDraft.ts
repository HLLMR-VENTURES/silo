// The Mod settings dialog's drafts (#358): every settings file of a mod has its own form values and
// raw XML text. Field ids restart at 0 in each file, so a value is keyed by file and id. Saving one
// file re-reads that file only: the other files' unsaved edits stay, and so does an edit made to
// the saved file while its write was out.
//   let d = draftsOf(files);
//   const sent = snapshotOf(d, file.path, wroteRaw); // right before the write
//   d = afterSave(d, await getModSettings(mod), file.path, sent);
import type { SettingsFile } from "./types";

/** The dialog's state: the files as last read, and the user's drafts. */
export interface SettingsDrafts {
  files: SettingsFile[];
  /** fieldKey(path, id) → the form's current value. */
  values: Record<string, string>;
  /** path → the raw editor's current text. */
  rawText: Record<string, string>;
}

/** What one file's drafts were when its save was sent. */
export interface SentDrafts {
  values: Record<string, string>;
  /** The raw text the file holds after this write as far as the drafts go: the text sent (a raw
   *  save), or the last reading (a form save). A raw draft that differs from it is kept. */
  raw: string | undefined;
}

/**
 * The key of one form field: ids are per file, so two files' field 0 are different fields.
 * @param path the settings file
 * @param id the field's id in that file
 */
export function fieldKey(path: string, id: number): string {
  return `${path}\u0000${id}`;
}

/**
 * Fresh drafts for files just read: every value and raw text as the file has it.
 * @param files the mod's settings files
 */
export function draftsOf(files: SettingsFile[]): SettingsDrafts {
  const values: Record<string, string> = {};
  for (const f of files) for (const fld of f.fields) values[fieldKey(f.path, fld.id)] = fld.value;
  return { files, values, rawText: Object.fromEntries(files.map((f) => [f.path, f.raw])) };
}

/**
 * One file's drafts as they are now (taken right before its save is sent).
 * @param d the dialog's state
 * @param path the file being saved
 * @param wroteRaw the save writes the raw text (else form edits: an unsaved raw draft stays)
 */
export function snapshotOf(d: SettingsDrafts, path: string, wroteRaw: boolean): SentDrafts {
  const file = d.files.find((f) => f.path === path);
  const values: Record<string, string> = {};
  for (const fld of file?.fields ?? []) {
    const k = fieldKey(path, fld.id);
    if (k in d.values) values[k] = d.values[k];
  }
  return { values, raw: wroteRaw ? d.rawText[path] : file?.raw };
}

/**
 * The state after `path` was saved and the mod's files read again. Only that file takes the new
 * reading; each of its drafts goes back to what the file now holds unless the user changed it
 * after the save was sent. Every other file keeps its drafts (and its last reading, which its
 * Save compares against).
 * @param d the state now
 * @param fresh the mod's settings files, read after the save
 * @param path the file that was saved
 * @param sent that file's drafts when the save was sent
 */
export function afterSave(d: SettingsDrafts, fresh: SettingsFile[], path: string, sent: SentDrafts): SettingsDrafts {
  const read = fresh.find((f) => f.path === path);
  if (!read) return d; // gone since: leave the dialog as it is (the next open reads it again)
  const values: Record<string, string> = {};
  const prefix = `${path}\u0000`;
  for (const [k, v] of Object.entries(d.values)) if (!k.startsWith(prefix)) values[k] = v;
  for (const fld of read.fields) {
    const k = fieldKey(path, fld.id);
    const editedSince = k in sent.values && k in d.values && d.values[k] !== sent.values[k];
    values[k] = editedSince ? d.values[k] : fld.value;
  }
  const rawEditedSince = sent.raw !== undefined && d.rawText[path] !== undefined && d.rawText[path] !== sent.raw;
  return {
    files: d.files.map((f) => (f.path === path ? read : f)),
    values,
    rawText: { ...d.rawText, [path]: rawEditedSince ? d.rawText[path] : read.raw },
  };
}

/**
 * After a form save that was written but couldn't be read back: the file holds what was sent, so
 * that is the saved value each field is compared with (an edit back to the old value while the
 * write was out still reads as unsaved). Drafts are kept as they are.
 * @param d the state now
 * @param path the file that was saved
 * @param sent that file's drafts when the save was sent
 */
export function markSent(d: SettingsDrafts, path: string, sent: SentDrafts): SettingsDrafts {
  return {
    ...d,
    files: d.files.map((f) =>
      f.path !== path
        ? f
        : { ...f, fields: f.fields.map((fld) => ({ ...fld, value: sent.values[fieldKey(path, fld.id)] ?? fld.value })) },
    ),
  };
}
