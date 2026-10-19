// The BulkBar's Turn on / Park as one sequence (#242), with everything that touches the app
// injected so each branch unit-tests:
//   1. work out which selected mods change (the ones already right are left out);
//   2. refuse at once, before any dialog, while another library write runs;
//   3. ask: parking loose mods (ones Silo hasn't filed yet) in a bulk action always asks, saying
//      it files them into the library, and that one dialog also covers the >25 question; a
//      single loose mod gets the one-time explanation (parkLoose.ts); anything else asks only
//      over 25 (bulk.ts);
//   4. file the loose mods first (their own library write); refused, stop: nothing changed;
//   5. then switch the rest (another library write);
//   6. offer Undo whenever something ran, warning when part of it didn't.
//   const outcome = await runBulkSwitch(selectedMods, false, deps);
import type { ConfirmOptions } from "../../ui/dialogs.svelte";
import type { ToastOptions, ToastTone } from "../../ui/toasts.svelte";
import type { SwitchUndoOffer } from "../../state/switchUndo";
import { switchMessage } from "../../state/switchUndo";
import { modsCount } from "../../archiveMoveLogic";
import { BULK_CONFIRM_OVER, bulkChanges, confirmBulkChange } from "./bulk";

/** The parts of a mod the bulk switch reads. */
export interface BulkMod {
  techName: string;
  title?: string | null;
  category: string;
}

/** How a projected switch went: true clean, false with problems (reported), null refused
 *  (another write; the refusal was shown), "failed" when it threw (reported). */
export type ProjectedResult = boolean | null | "failed";

export interface BulkSwitchDeps<M extends BulkMod> {
  /** The active set now (read again after each step). */
  active: () => ReadonlySet<string>;
  /** A loose mod: set_active can't park it, so parking files it into the library. */
  isLoose: (m: M) => boolean;
  /** Why a library write can't start now (the refusal to show), or null. */
  busy: () => string | null;
  /** The mods folder is cloud-synced (each mod is a copy, not a link). */
  cloudSynced: boolean;
  /** The library folder is on another drive than the mods folder. */
  archiveOnOtherDrive: boolean;
  /** The path separator the dialogs show ("\\" on Windows). */
  sep?: string;
  /** The in-app confirm (ui's ask). */
  ask: (opts: ConfirmOptions) => Promise<boolean>;
  /** The one-time explanation before parking one loose mod (parkLoose.ts); false = cancelled. */
  explain: (name: string, category: string) => Promise<boolean>;
  /**
   * File these loose mods into the library, built fresh from the scan at the time (the ones no
   * longer loose are skipped). Takes its own library write and dismisses the previous switch toast
   * once it starts.
   * @returns true clean, false with problems (reported), null refused (the refusal was shown)
   */
  fileLoose: (techNames: string[]) => Promise<boolean | null>;
  /** How many of these are still loose (after a filing with problems). */
  stillLoose: (techNames: string[]) => number;
  /** Turn these on or park them through the projection (its own library write). */
  applyProjected: (techNames: string[], on: boolean) => Promise<ProjectedResult>;
  toast: (opts: ToastOptions) => unknown;
  /** The switch toast with Undo. */
  offerUndo: (o: SwitchUndoOffer) => unknown;
}

/** What the bulk switch did, for callers and tests. */
export type BulkSwitchOutcome =
  | "nothing" // every mod was already as asked
  | "busy" // another write was running: refused before asking
  | "cancelled" // the user said no
  | "refused" // a write was refused after the dialog: nothing changed
  | "failed" // the switch threw and nothing else ran (reported)
  | "done"; // something ran: Undo was offered

/** The most loose mods the filing dialog lists by name. */
export const LOOSE_LIST_MAX = 50;

const nameOf = (m: { techName: string; title?: string | null }) => m.title ?? m.techName;

/**
 * The drive (or network share) a path is on, or null when it can't tell (a POSIX path).
 * @param p a folder path ("D:\\Mods", "\\\\nas\\share\\mods")
 */
export function driveOf(p: string): string | null {
  const letter = /^([A-Za-z]):(?:[\\/]|$)/.exec(p);
  if (letter) return `${letter[1].toUpperCase()}:`;
  const unc = /^[\\/]{2}([^\\/]+)[\\/]([^\\/]+)/.exec(p);
  if (unc) return `\\\\${unc[1]}\\${unc[2]}`.toLowerCase();
  return null;
}

/**
 * True when the library folder is on another drive than the mods folder (both known).
 * @param archive the library archive's folder (null for the default one inside the mods folder)
 * @param mods the mods folder
 */
export function onOtherDrive(archive: string | null | undefined, mods: string | null | undefined): boolean {
  if (!archive || !mods) return false;
  const a = driveOf(archive);
  const b = driveOf(mods);
  return a !== null && b !== null && a !== b;
}

/**
 * The dialog before a bulk Park that files loose mods (it also stands in for the >25 question).
 * @param count how many mods the Park changes
 * @param loose the loose ones among them (filed into the library)
 * @param opts `otherDrive`: the library folder is on another drive; `cloudSynced`: the mods
 *   folder is cloud-synced; `sep`: the path separator to show
 */
export function bulkParkLooseConfirm(
  count: number,
  loose: readonly BulkMod[],
  opts: { otherDrive?: boolean; cloudSynced?: boolean; sep?: string } = {},
): ConfirmOptions {
  const n = modsCount(count);
  const l = loose.length;
  const categories = new Set(loose.map((m) => m.category));
  const where =
    categories.size === 1
      ? `(its ${[...categories][0]} folder)` // the category only: the archive may be anywhere
      : l === 1
        ? "(its category folder in your library archive)"
        : "(each into its category folder in your library archive)";
  const who =
    l === count
      ? l === 1
        ? "It isn't filed yet, so Silo moves it"
        : "They aren't filed yet, so Silo moves them"
      : l === 1
        ? "1 of them isn't filed yet, so Silo moves it"
        : `${l} of them aren't filed yet, so Silo moves those`;
  let body = `${who} into your library archive ${where}. Nothing is deleted. Undo puts ${l === 1 && count === 1 ? "it" : "them"} back.`;
  if (count > BULK_CONFIRM_OVER && opts.cloudSynced)
    body += " Your mods folder is cloud-synced, so each one is moved or removed on its own. This can take a while.";
  const facts = [
    { label: "Park", value: n },
    { label: "Filed into your library first", value: modsCount(l) },
  ];
  if (opts.otherDrive) facts.push({ label: "Library folder", value: "On another drive" });
  const list = loose.slice(0, LOOSE_LIST_MAX).map(nameOf);
  if (l > LOOSE_LIST_MAX) list.push(`…and ${l - LOOSE_LIST_MAX} more`);
  return { title: `Park ${n}?`, body, facts, list, confirm: `Park ${n}` };
}

/**
 * The switch toast's message and tone once everything ran.
 * @param names the titles of every mod that was meant to change
 * @param on true for Turn on
 * @param r `unfiled`: loose mods still loose after a filing with problems; `skipped`: mods the
 *   second step didn't change (refused or failed); `problems`: some step finished with problems
 */
export function bulkResultMessage(
  names: string[],
  on: boolean,
  r: { unfiled: number; skipped: number; problems: boolean },
): { message: string; tone: ToastTone } {
  const missed: string[] = [];
  if (r.unfiled > 0) missed.push(`${r.unfiled} couldn't be filed`);
  if (r.skipped > 0) missed.push(`${r.skipped} ${r.skipped === 1 ? "wasn't" : "weren't"} changed`);
  if (missed.length > 0) {
    const done = names.length - r.unfiled - r.skipped;
    const head = `${on ? "Turned on" : "Parked"} ${modsCount(Math.max(0, done))}`;
    return { message: `${head}, but ${missed.join(" and ")}.`, tone: "warn" };
  }
  if (r.problems) return { message: switchMessage(names, on, false), tone: "warn" };
  return { message: switchMessage(names, on, true), tone: "neutral" };
}

/**
 * Turn on or park the selected mods (the BulkBar's Turn on / Park).
 * @param list the selected mods
 * @param on true for Turn on, false for Park
 * @param deps the app's dialogs, writes and toasts
 * @returns what happened (see BulkSwitchOutcome)
 */
export async function runBulkSwitch<M extends BulkMod>(
  list: readonly M[],
  on: boolean,
  deps: BulkSwitchDeps<M>,
): Promise<BulkSwitchOutcome> {
  const changing = bulkChanges(list, on, deps.active());
  if (changing.length === 0) {
    deps.toast({
      message: list.length === 1 ? (on ? "It's already on." : "It's already parked.") : on ? "They're already on." : "They're already parked.",
    });
    return "nothing";
  }
  const why = deps.busy();
  if (why) {
    deps.toast({ message: why, tone: "warn" });
    return "busy";
  }
  // Loose mods can't be parked by set_active (they're the user's own files): they're filed.
  const loose = on ? [] : changing.filter(deps.isLoose);
  const bulk = list.length > 1;
  if (loose.length > 0 && bulk) {
    // Moving files the user put there: always say so in a bulk action ("Don't explain this
    // again" only covers parking one mod). One dialog, the >25 question included.
    const opts = bulkParkLooseConfirm(changing.length, loose, {
      otherDrive: deps.archiveOnOtherDrive,
      cloudSynced: deps.cloudSynced,
      sep: deps.sep,
    });
    if (!(await deps.ask(opts))) return "cancelled";
  } else {
    if (!(await confirmBulkChange(changing.length, on, deps.cloudSynced, deps.ask))) return "cancelled";
    if (loose.length > 0 && !(await deps.explain(nameOf(loose[0]), loose[0].category))) return "cancelled";
  }

  const previous = [...deps.active()];
  const looseNames = loose.map((m) => m.techName);
  let filed: boolean | null = null;
  let unfiled = 0;
  if (looseNames.length > 0) {
    filed = await deps.fileLoose(looseNames);
    if (filed === null) return "refused"; // nothing changed; the refusal was shown
    if (filed === false) unfiled = deps.stillLoose(looseNames);
  }

  const looseSet = new Set(looseNames);
  const projected = changing.filter((m) => !looseSet.has(m.techName)).map((m) => m.techName);
  let skipped = 0;
  let problems = filed === false;
  if (projected.length > 0) {
    const r = await deps.applyProjected(projected, on);
    if (r === null || r === "failed") {
      // Nothing filed before it: nothing to undo.
      if (filed === null) return r === null ? "refused" : "failed";
      skipped = projected.length;
    } else if (!r) {
      problems = true;
    }
  }

  const { message, tone } = bulkResultMessage(changing.map(nameOf), on, { unfiled, skipped, problems });
  deps.offerUndo({ message, tone, previous, expected: [...deps.active()] });
  return "done";
}
