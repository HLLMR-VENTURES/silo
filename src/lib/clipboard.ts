// Copy text to the clipboard, or save it to a file when the clipboard can't be used, so the
// user always ends up with the text somewhere.

/**
 * Copy `text`, falling back to saving it as `fallbackName`.
 * @param text what to copy
 * @param fallbackName the file name offered when copying fails
 * @param copy writes the clipboard (Silo's `copyText`, done in Rust)
 * @param save asks for a path and saves (Silo's `saveTextFile`); false when cancelled
 * @returns "copied", "saved", or "cancelled" (copy failed and the save was cancelled)
 */
export async function copyOrSave(
  text: string,
  fallbackName: string,
  copy: (text: string) => Promise<void>,
  save: (name: string, text: string) => Promise<boolean>,
): Promise<"copied" | "saved" | "cancelled"> {
  try {
    await copy(text);
    return "copied";
  } catch {
    return (await save(fallbackName, text)) ? "saved" : "cancelled";
  }
}
