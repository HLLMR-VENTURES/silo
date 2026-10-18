// Parking a loose mod (a .zip in the mods folder that Silo hasn't filed yet) files it into the
// library archive: the game stops loading it and it stays in the library. The first time, Silo
// explains that and asks; "Don't explain this again" (kept in `silo.explain.parkLoose`) stops
// the question for good, and once answered it isn't asked again in the same session either.
//   const explainPark = createParkLooseExplainer(askWithOption);
//   if (!(await explainPark("Courseplay", "Scripts & Tools"))) return; // cancelled
import type { ConfirmOptions, ConfirmWithOption } from "../../ui/dialogs.svelte";

export const PARK_LOOSE_KEY = "silo.explain.parkLoose";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

/**
 * The explanation dialog.
 * @param name the mod's title, or a count ("3 mods") for several
 * @param category the archive folder it's filed under (its category)
 * @param sep the path separator to show ("\\" on Windows)
 */
export function parkLooseOptions(name: string, category: string, sep = "\\"): ConfirmOptions {
  return {
    title: `Park ${name}?`,
    body:
      `Silo moves it into your library folder (archive${sep}${category}) so the game stops loading it. ` +
      "It stays in your library. Switch it on any time.",
    confirm: "Park it",
    checkbox: { label: "Don't explain this again" },
  };
}

/**
 * The check that runs before parking a loose mod.
 * @param askWithOption the dialog (ui's askWithOption)
 * @param storage where "don't explain" is kept (defaults to localStorage)
 * @param sep the path separator the explanation shows
 * @returns explain(name, category): true to go ahead (explained and confirmed, or nothing to
 *   explain), false when the user cancelled
 */
export function createParkLooseExplainer(
  askWithOption: (opts: ConfirmOptions) => Promise<ConfirmWithOption>,
  storage: StorageLike | null = defaultStorage(),
  sep = "\\",
): (name: string, category: string) => Promise<boolean> {
  let explained = false;
  const dismissed = () => {
    try {
      return storage?.getItem(PARK_LOOSE_KEY) === "1";
    } catch {
      return false;
    }
  };
  return async (name, category) => {
    if (explained || dismissed()) return true;
    const { confirmed, checked } = await askWithOption(parkLooseOptions(name, category, sep));
    if (!confirmed) return false;
    explained = true;
    if (checked) {
      try {
        storage?.setItem(PARK_LOOSE_KEY, "1");
      } catch {
        // Not kept: it's explained once per session instead.
      }
    }
    return true;
  };
}
