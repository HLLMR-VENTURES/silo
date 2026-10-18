// Automatic mod update checks at startup: off by default, offered once on the first visit to
// Updates, changeable in Settings → General. The check sends installed mods' names to Silo's
// catalog, so it never runs unless the user turned it on.
//   if (takeAutoCheckOffer()) autoCheckOffer.open = true;   // the Updates view, on visit
//   answerAutoCheckOffer(true);                             // "Check on start"
//   if (autoCheckEnabled()) checkAllUpdates(true);          // startup, after the first scan
// Both values persist in localStorage (silo.updates.autoCheck, silo.updates.autoCheckOffered).
// Storage can be missing or throw (private mode, tests): reads fall back to "off, not offered".

export const AUTO_CHECK_KEY = "silo.updates.autoCheck";
export const AUTO_CHECK_OFFERED_KEY = "silo.updates.autoCheckOffered";

/** The privacy line shown wherever the check is offered or run. */
export const UPDATE_PRIVACY_LINE =
  "Checking sends the names of your installed mods to Silo's catalog. Versions are compared on your computer.";

function read(key: string): string | null {
  try {
    return typeof localStorage !== "undefined" ? localStorage.getItem(key) : null;
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(key, value);
  } catch {
    // Not persisted; this session still follows the choice through autoCheck.on.
  }
}

/** The setting, reactive for Settings → General. Change it through setAutoCheck. */
export const autoCheck = $state({ on: read(AUTO_CHECK_KEY) === "on" });

/** Whether the offer card is showing in the Updates view (this session). */
export const autoCheckOffer = $state({ open: false });

/** True when the user turned on checking at startup. Off unless they did. */
export function autoCheckEnabled(): boolean {
  return autoCheck.on;
}

/**
 * Turn checking at startup on or off. Counts as answering the offer.
 * @param on the new setting
 */
export function setAutoCheck(on: boolean): void {
  autoCheck.on = on;
  write(AUTO_CHECK_KEY, on ? "on" : "off");
  write(AUTO_CHECK_OFFERED_KEY, "1");
}

/**
 * The first visit to Updates: true exactly once per install (it records that the offer was
 * made), false after that, and false when the setting was already chosen in Settings.
 */
export function takeAutoCheckOffer(): boolean {
  if (read(AUTO_CHECK_OFFERED_KEY) === "1" || read(AUTO_CHECK_KEY) !== null) return false;
  write(AUTO_CHECK_OFFERED_KEY, "1");
  return true;
}

/**
 * The offer card's buttons: "Check on start" (true) or "Only when I ask" (false).
 * @param on the choice
 */
export function answerAutoCheckOffer(on: boolean): void {
  setAutoCheck(on);
  autoCheckOffer.open = false;
}

/** Test hook: re-read storage into the reactive setting. */
export function reloadAutoCheck(): void {
  autoCheck.on = read(AUTO_CHECK_KEY) === "on";
  autoCheckOffer.open = false;
}
