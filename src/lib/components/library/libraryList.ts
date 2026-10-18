// The Library's search and sort, kept pure so the 2,500-mod perf smoke test can drive it (#258).
//   searchMods(list, "lizard")             // title, tech name or author contains the words
//   sortMods(list, "name", "asc", rating)  // ties break by name
// Lowercased search text and sort names are computed once per scan entry (a WeakMap on the entry
// object, so a rescan's fresh entries start clean), not per keystroke or per comparison; sorting
// decorates once and compares with shared Intl.Collators instead of localeCompare per call.
import type { ModEntry } from "../../types";

export type SortKey = "name" | "category" | "size" | "added" | "version" | "rating";

type SortMod = Pick<ModEntry, "techName" | "title" | "author" | "size" | "mtimeMs" | "version" | "category" | "subcategory">;

/** Same ordering as `a.localeCompare(b)` (the default locale and options). */
const text = new Intl.Collator();
/** Same ordering as `a.localeCompare(b, undefined, { numeric: true })`: "1.10" after "1.9". */
const numeric = new Intl.Collator(undefined, { numeric: true });

const searchKeys = new WeakMap<object, string>();
const nameKeys = new WeakMap<object, string>();

/** The text a search looks in: title, tech name and author, lowercased, one per line. */
export function searchKey(m: Pick<ModEntry, "techName" | "title" | "author">): string {
  let k = searchKeys.get(m);
  if (k === undefined) {
    k = `${(m.title ?? "").toLowerCase()}\n${m.techName.toLowerCase()}\n${(m.author ?? "").toLowerCase()}`;
    searchKeys.set(m, k);
  }
  return k;
}

/** The name a mod sorts by: its title (or tech name), lowercased. */
export function nameKey(m: Pick<ModEntry, "techName" | "title">): string {
  let k = nameKeys.get(m);
  if (k === undefined) {
    k = (m.title ?? m.techName).toLowerCase();
    nameKeys.set(m, k);
  }
  return k;
}

/**
 * The mods whose title, tech name or author contains `query` (case-insensitive).
 * @param list the mods to search
 * @param query already trimmed and lowercased; "" returns `list` itself
 */
export function searchMods<M extends Pick<ModEntry, "techName" | "title" | "author">>(list: readonly M[], query: string): readonly M[] {
  if (query === "") return list;
  // A newline can't come from the search field, so a match never spans two fields.
  return list.filter((m) => searchKey(m).includes(query));
}

/**
 * A sorted copy of `list`.
 * @param list the mods (not changed)
 * @param by the sort key
 * @param dir ascending or descending (the name tie-break always runs A–Z)
 * @param rating the user's star rating for a tech name (for "rating")
 */
export function sortMods<M extends SortMod>(
  list: readonly M[],
  by: SortKey,
  dir: "asc" | "desc",
  rating: (techName: string) => number,
): M[] {
  const mul = dir === "desc" ? -1 : 1;
  const rows = list.map((m) => ({ m, name: nameKey(m), r: by === "rating" ? rating(m.techName) : 0 }));
  rows.sort((a, b) => {
    let r = 0;
    switch (by) {
      case "name":
        r = text.compare(a.name, b.name);
        break;
      case "size":
        r = a.m.size - b.m.size;
        break;
      case "added":
        r = a.m.mtimeMs - b.m.mtimeMs;
        break;
      case "version":
        r = numeric.compare(a.m.version ?? "", b.m.version ?? "");
        break;
      case "rating":
        // My own star rating (0 = unrated sorts last on desc, which is the default use).
        r = a.r - b.r;
        break;
      case "category":
        r =
          text.compare(a.m.category, b.m.category) ||
          text.compare(a.m.subcategory ?? "", b.m.subcategory ?? "") ||
          text.compare(a.name, b.name);
        break;
    }
    return r * mul || text.compare(a.name, b.name);
  });
  return rows.map((row) => row.m);
}
