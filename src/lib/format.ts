// Small, pure formatting helpers shared across components.

/** Human-readable byte size (e.g. "1.2 GB", "512 MB"). */
export function fmtSize(b: number): string {
  if (b >= 1024 ** 3) return (b / 1024 ** 3).toFixed(1) + " GB";
  if (b >= 1024 ** 2) return (b / 1024 ** 2).toFixed(0) + " MB";
  if (b >= 1024) return (b / 1024).toFixed(0) + " KB";
  return b + " B";
}

/**
 * A version as Silo shows it: catalogs and GitHub tags may carry "v", "V", "V." or "Ver" in front
 * ("V.1.6.4.0", "Ver 2.1"); scan entries don't. Silo shows the bare number everywhere and never
 * adds a "v". Updates compares versions in this form too (normVersion), so shown and compared agree.
 *   displayVersion("V.1.6.4.0") -> "1.6.4.0"
 * @param v the version as stored or received ("" when there is none)
 */
export function displayVersion(v: string | null | undefined): string {
  return (v ?? "").trim().replace(/^(v|ver|version)\.?\s*(?=\d)/i, "");
}

/**
 * A mod's size for display, or null when there's nothing to show: Silo doesn't measure folder
 * (unpacked) mods, whose scan size is 0, and "0 B" would be wrong.
 * @param m the mod's kind and scanned size in bytes
 */
export function modSizeLabel(m: { kind: string; size: number }): string | null {
  if (m.kind !== "zip" && m.size <= 0) return null;
  return fmtSize(m.size);
}
