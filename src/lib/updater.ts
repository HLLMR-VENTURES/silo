// Self-update via the Tauri updater plugin. The desktop app checks GitHub Releases for a
// newer signed build (latest.json), and can download + install it, then relaunch. The
// updater is a nicety, never a blocker: if there's no signed release, we're offline, or
// this is a dev build, checkUpdate() just returns null and the UI shows nothing.

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export type AvailableUpdate = { version: string };

// The resolved Update handle from the last successful check, held so installUpdate() can act
// on exactly what checkUpdate() found.
let pending: Update | null = null;

/**
 * Check for an available update.
 * @returns the new version if one is available, else null (also null on any error —
 *          offline, no endpoint/release yet, or running under `tauri dev`).
 */
export async function checkUpdate(): Promise<AvailableUpdate | null> {
  try {
    const update = await check();
    if (update) {
      pending = update;
      return { version: update.version };
    }
  } catch {
    // No signed release, no network, or dev build — updates are optional, so stay quiet.
  }
  pending = null;
  return null;
}

/**
 * Download the pending update, then (if `mayInstall` still agrees) install it and relaunch into
 * the new version. Downloading changes nothing; `mayInstall` runs right before the install, so a
 * change Silo started meanwhile can still stop it.
 * @param onProgress percent 0–100, or null when the total size is unknown.
 * @param mayInstall resolves false to stop before installing.
 * @returns false if `mayInstall` stopped it (nothing was installed).
 */
export async function installUpdate(
  onProgress: (pct: number | null) => void,
  mayInstall: () => Promise<boolean>,
): Promise<boolean> {
  if (!pending) return true;
  let total = 0;
  let got = 0;
  await pending.download((ev) => {
    if (ev.event === "Started") {
      total = ev.data.contentLength ?? 0;
      onProgress(total ? 0 : null);
    } else if (ev.event === "Progress") {
      got += ev.data.chunkLength;
      onProgress(total ? Math.min(100, Math.round((got / total) * 100)) : null);
    } else if (ev.event === "Finished") {
      onProgress(100);
    }
  });
  if (!(await mayInstall())) return false;
  await pending.install();
  await relaunch();
  return true;
}
