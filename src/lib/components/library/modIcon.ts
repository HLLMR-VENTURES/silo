// Mod icons for library rows and cards, cached for the session so scrolling back doesn't
// re-request one already loaded (the Rust side also disk-caches the decoded PNGs).
//   const url = cachedModIcon(mod) ?? (await loadModIcon(mod));
import { getModIcon } from "../../api";
import type { ModEntry } from "../../types";

const cache = new Map<string, string>();
const pending = new Map<string, Promise<string | null>>();

type IconMod = Pick<ModEntry, "path" | "kind" | "iconFilename">;

/** The icon's data URL if it was loaded this session. */
export function cachedModIcon(mod: IconMod): string | null {
  return cache.get(mod.path) ?? null;
}

/**
 * Load a mod's icon (once per path, shared by every row and card asking at the same time).
 * @returns the data URL, or null when the mod has none or it can't be decoded
 */
export function loadModIcon(mod: IconMod): Promise<string | null> {
  if (!mod.iconFilename) return Promise.resolve(null);
  const hit = cache.get(mod.path);
  if (hit) return Promise.resolve(hit);
  let p = pending.get(mod.path);
  if (!p) {
    p = getModIcon(mod.path, mod.kind, mod.iconFilename)
      .then((url) => {
        if (url) cache.set(mod.path, url);
        return url;
      })
      .catch(() => null)
      .finally(() => pending.delete(mod.path));
    pending.set(mod.path, p);
  }
  return p;
}
