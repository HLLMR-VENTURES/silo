// Small display helpers for the mod drawer. Pure.

/** "1.2.0.0" or "v1.2.0.0" -> "v1.2.0.0" (never "vv…"). */
export function versionLabel(v: string): string {
  return "v" + v.trim().replace(/^v/i, "");
}

/** Bytes as a short size: 1536 -> "2 KB", 5.2e9 -> "4.8 GB". */
export function formatSize(b: number): string {
  if (b >= 1024 ** 3) return (b / 1024 ** 3).toFixed(1) + " GB";
  if (b >= 1024 ** 2) return (b / 1024 ** 2).toFixed(0) + " MB";
  if (b >= 1024) return (b / 1024).toFixed(0) + " KB";
  return b + " B";
}

/**
 * True only when `latest` is strictly newer than `current` (segment-wise numeric compare, same
 * rule as the Rust `github::is_newer`). A different or older version is not an update.
 */
export function versionNewer(latest: string, current: string): boolean {
  const seg = (s: string) =>
    s
      .replace(/^v/i, "")
      .split(/[.\-+_]/)
      .map((p) => parseInt(p, 10))
      .filter((n) => Number.isFinite(n));
  const a = seg(latest);
  const b = seg(current);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i] ?? 0;
    const y = b[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}
