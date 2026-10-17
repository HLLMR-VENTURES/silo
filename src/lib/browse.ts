// Pure helpers shared across the Browse-tab components (card grid + detail drawer).

import type { CatalogModDetail } from "./types";
import { catalogImage } from "./api";

// GIANTS' ModHub CDN refuses requests that don't come from its own site. Silo respects
// that: a thumbnail still pointing there (not yet cached by SiloAPI) shows the placeholder.
const MODHUB_CDN = /(?:giants-software\.com|farming-simulator\.com)/i;
// GitHub-hosted images go through the Rust proxy (the CSP keeps the webview off them).
const GITHUB_HOSTED = /(?:githubusercontent\.com|github\.com)/i;

// Image loading. SiloAPI caches catalog images server-side and hands back an `imageUrl` on
// its own host — those load DIRECTLY as a plain <img> (CSP allows silo-api). GitHub images
// go through the Rust proxy, which returns a data: URL. Dedupe in-session so paging back
// doesn't refetch.
const imageCache = new Map<string, Promise<string>>();
export function loadCatalogImage(url: string): Promise<string> {
  if (MODHUB_CDN.test(url)) return Promise.resolve("");
  // Cached SiloAPI URL (or any other allowed host): the browser can load and cache it itself.
  if (!GITHUB_HOSTED.test(url)) return Promise.resolve(url);
  let p = imageCache.get(url);
  if (!p) {
    p = catalogImage(url).catch(() => "");
    imageCache.set(url, p);
  }
  return p;
}

export const SOURCE_LABEL: Record<string, string> = {
  github: "GitHub",
  modhub: "ModHub",
};

// Compact codes for the card's source buttons, where space is tight.
export const SOURCE_SHORT: Record<string, string> = {
  github: "GH",
  modhub: "MH",
};

export const shortLabel = (s: string): string => SOURCE_SHORT[s] ?? s;
export const label = (s: string): string => SOURCE_LABEL[s] ?? s;

/** Why a source can't be installed directly — shown on hover and in the drawer. */
export function gatedReason(source: string): string {
  if (source === "modhub")
    return "ModHub mods download from GIANTS' website, so Silo opens the mod's page there.";
  return "This source doesn't allow direct downloads. Opens the mod page.";
}

export function fmtMB(bytes: number): string {
  return (bytes / (1024 * 1024)).toFixed(1);
}

/** Compact download counts: 980, 12k, 1.3M. */
export function fmtCount(n: number): string {
  if (n < 1000) return `${n}`;
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

/** Pull owner/repo out of a github.com source URL, for the interactive card. */
export function parseRepo(url: string): { owner: string; repo: string } | null {
  const m = url.match(/github\.com\/([^/]+)\/([^/?#]+)/i);
  if (!m) return null;
  return { owner: m[1], repo: m[2].replace(/\.git$/i, "") };
}

/** Whether a "Read more" affordance is worth showing: an ingested full body, or a
 *  summary long enough that a dedicated modal helps. */
export function canExpand(d: CatalogModDetail): boolean {
  return !!d.descriptionFull || (d.description != null && d.description.length > 160);
}
