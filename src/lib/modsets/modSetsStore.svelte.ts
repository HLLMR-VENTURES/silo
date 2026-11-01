// The mod sets App holds: the saved sets, the remembered last-applied set, and every action the
// Mod sets page, the Play menu and the Savegames tab run. Switching goes through App's guarded
// `setActiveSetTo` and its switch Undo toast; App passes those in.
//   const setStore = createModSets({ active: () => activeSet, setActiveSetTo, undo: switchUndo, ... });
//   await setStore.load();
//   setStore.state           // { kind: "custom", name: "Harvest 2026", changes: 3, ... }
//   await setStore.switchTo(set);
import { ask, askText, toast, type ConfirmFact } from "../ui";
import { reportError } from "../state/notify.svelte";
import { deleteLoadout, getLoadouts, replaceLoadoutMods, saveAutoSnapshot, saveLoadout } from "../api";
import { activeSetDiff, findLoadoutByName, loadoutNameCheck, sameMods, type ActiveSetDiff } from "../loadoutSafety";
import { formatCount, plural } from "../plural";
import { switchMessage, type SwitchUndo } from "../state/switchUndo";
import type { Loadout, Savegame } from "../types";
import {
  activeSetState,
  afterSwitch,
  loadLastApplied,
  savedAs,
  saveModStats,
  snapshotNameFor,
  storeLastApplied,
  type ActiveSetState,
  type LastApplied,
} from "./modSets";
import { activeChange, membershipMessage, type MembershipEdit } from "./membership";

export interface ModSetsDeps {
  /** Tech names active now. */
  active: () => ReadonlySet<string>;
  /** Tech names in the library, or undefined before the first scan (nothing counts as missing). */
  library: () => ReadonlySet<string> | undefined;
  /** A mod's display title. */
  title: (techName: string) => string;
  /** App's guarded switch: true = clean, false = finished with problems (reported), null =
   *  nothing changed: refused because another write or a scan runs (said so), cancelled, or it
   *  failed (reported) and the mods folder still holds the old set. */
  setActiveSetTo: (mods: string[], label: string) => Promise<boolean | null>;
  /** Wait briefly for a running library scan, so the active mods read next aren't about to be
   *  replaced by it. False when it still runs (refused, said why). */
  settle: () => Promise<boolean>;
  /** App's switch Undo toast (one at a time, with the staleness guard). */
  undo: SwitchUndo;
  /** Saving, replacing and deleting sets wait for a foreground library change. True when it
   *  refused (and said why). */
  editRefused: () => boolean;
  /** Say why something can't happen (a warn toast). */
  refuse: (why: string) => void;
  /** Why an Undo that puts the active mods back can't run now (App's activeUndoBlocked), or null.
   *  @param expected the active mods the change left */
  activeUndoBlocked: (expected: string[]) => string | null;
  /** Park these mods through the normal Park path (the "Mod sets…" toast's "Park them too"). */
  park: (techNames: string[]) => unknown;
}

export interface ModSets {
  readonly sets: Loadout[];
  readonly lastApplied: LastApplied | null;
  /** Which set the active mods are on. */
  readonly state: ActiveSetState;
  /** The set `state` names (the matching or last-applied set), if it still exists. */
  readonly current: Loadout | undefined;
  load(): Promise<void>;
  /** Confirm with the diff, switch, save a "Before …" snapshot, offer Undo. */
  switchTo(set: Loadout): Promise<void>;
  /** "Save current mods as a set" (name prompt; a duplicate name offers Replace). */
  saveCurrent(): Promise<void>;
  /** "Update from current mods": replace a set's mods with the active ones, with Undo. */
  updateFromCurrent(set: Loadout): Promise<void>;
  /** Delete a set, with Undo. */
  remove(set: Loadout): Promise<void>;
  /** "Use this save's mods": the same switch confirmation, saving a set named after the save. */
  useSave(save: Savegame): Promise<void>;
  /** Save a shared set's mods as a new set without downloading anything. Returns its id. */
  saveShared(name: string, mods: string[]): Promise<number | null>;
  /**
   * The Library's "Mod sets…" Save (#323): write every edited set at once. When the set you're on
   * is edited, its added mods turn on (or its removed ones are parked) first, so you stay on it.
   * The toast offers Undo, and "Park them too" when mods removed from sets are still on.
   * @param techNames the chosen mods
   * @param edits what to write (membership.ts)
   * @returns false when nothing was saved (refused, cancelled or failed): the dialog stays open
   */
  applyMembership(techNames: string[], edits: MembershipEdit[]): Promise<boolean>;
}

/** "1 mod" / "1,204 mods". */
function modsCount(n: number): string {
  return `${formatCount(n)} ${plural(n, { one: "mod", other: "mods" })}`;
}

/**
 * Body, facts and list for a "switch the active set" confirmation.
 * @param diff what the switch changes
 * @param title display title for a tech name
 * @param missingCount mods the target names that aren't in the library
 * @param snapshotName the "Before …" snapshot Silo saves, or null when it won't save one
 */
export function switchSummary(
  diff: ActiveSetDiff,
  title: (t: string) => string,
  missingCount = diff.missing.length,
  snapshotName: string | null = null,
): { body: string; facts: ConfirmFact[]; list: string[] } {
  const on = diff.on.length;
  const off = diff.off.length;
  const facts: ConfirmFact[] = [
    { label: "Turn on", value: modsCount(on) },
    { label: "Park", value: modsCount(off) },
    ...(missingCount ? [{ label: "Not in your library", value: modsCount(missingCount) }] : []),
  ];
  const changed = [...diff.on.map((t) => `Turn on: ${title(t)}`), ...diff.off.map((t) => `Park: ${title(t)}`)];
  const list = changed.slice(0, 50);
  if (changed.length > 50) list.push(`…and ${changed.length - 50} more`);
  // "693 mods turn on." / "5 mods are parked." / "2 mods turn on and 2 are parked." (no "0 are").
  const onPart = on > 0 ? `${modsCount(on)} ${plural(on, { one: "turns", other: "turn" })} on` : "";
  const offPart =
    off > 0 ? `${on > 0 ? formatCount(off) : modsCount(off)} ${plural(off, { one: "is", other: "are" })} parked` : "";
  let body = `${[onPart, offPart].filter(Boolean).join(" and ")}. Nothing is deleted.`;
  if (missingCount)
    body += plural(missingCount, {
      one: " 1 mod in this set isn't in your library, so it stays off until you get it.",
      other: ` ${modsCount(missingCount)} in this set aren't in your library, so they stay off until you get them.`,
    });
  if (snapshotName) body += ` Silo saves your current mods as “${snapshotName}” so you can switch back.`;
  return { body, facts, list };
}

/**
 * The mod sets store.
 * @param deps App's state and guarded paths
 */
export function createModSets(deps: ModSetsDeps): ModSets {
  let sets = $state<Loadout[]>([]);
  let last = $state<LastApplied | null>(loadLastApplied());

  function setLast(v: LastApplied | null) {
    last = v;
    storeLastApplied(v);
  }

  function currentState(): ActiveSetState {
    return activeSetState(sets, deps.active(), last, deps.library());
  }

  async function load() {
    try {
      sets = await getLoadouts();
    } catch (e) {
      reportError(e, "Loading your mod sets", { retry: load, background: true });
    }
  }

  /** Reload, and say so if it fails (after a change the user made). */
  async function reload() {
    try {
      sets = await getLoadouts();
    } catch (e) {
      reportError(e, "Loading your mod sets", { retry: load });
    }
  }

  /**
   * Switch the active set (already confirmed): apply, then save the "Before …" snapshot of what
   * was active, remember the new set, and offer Undo. Nothing is saved when the write was refused.
   * @param name what the toast calls it
   * @param mods the mods to make active
   * @param snapshotName the snapshot's name
   * @param target resolves the set switched to, after the switch applied (a savegame's set is
   *   saved only then)
   */
  async function perform(
    name: string,
    mods: string[],
    snapshotName: string,
    target: () => Promise<{ id: number; name: string; kind: Loadout["kind"] } | null>,
  ) {
    deps.undo.dismiss();
    // The snapshot and Undo keep the active mods from before: not while a scan may replace them.
    if (!(await deps.settle())) return;
    const previous = [...deps.active()];
    const was = currentState();
    const clean = await deps.setActiveSetTo(mods, `Switching to ${name}`);
    // Refused, or failed with nothing changed (said so): no snapshot, not remembered, no Undo.
    if (clean === null) return;
    // What the switch left (mods Silo couldn't park stay on), so Undo's staleness check holds.
    // Kept now: a scan landing during the saves below could bring changes made since.
    const left = [...deps.active()];
    let snapshotId: number | null = null;
    if (previous.length > 0) {
      try {
        snapshotId = await saveAutoSnapshot(snapshotName, previous);
      } catch (e) {
        reportError(e, "Saving your mods before the switch");
      }
    }
    try {
      const t = await target();
      if (t) setLast(afterSwitch(last, t, snapshotId, was));
    } catch (e) {
      reportError(e, `Saving ${name} as a set`);
    }
    await reload();
    deps.undo.offer({
      message: clean ? `Switched to ${name}.` : `Switch to ${name} finished with problems.`,
      tone: clean ? "neutral" : "warn",
      previous,
      expected: left,
    });
  }

  async function switchTo(set: Loadout) {
    // The confirmation's diff is from the active mods: not while a scan may replace them.
    if (!(await deps.settle())) return;
    const library = deps.library();
    const diff = activeSetDiff([...deps.active()], set.mods, library);
    if (diff.on.length + diff.off.length === 0) {
      setLast(afterSwitch(last, set, null, currentState()));
      toast({ message: `Already using ${set.name}.` });
      return;
    }
    const snapshotName = snapshotNameFor(set);
    const sum = switchSummary(diff, deps.title, diff.missing.length, deps.active().size ? snapshotName : null);
    if (
      !(await ask({
        title: set.kind === "auto" ? `Switch back to ${set.name}?` : `Switch to ${set.name}?`,
        body: sum.body,
        facts: sum.facts,
        list: sum.list,
        confirm: set.kind === "auto" ? "Switch back" : "Switch",
      }))
    )
      return;
    await perform(set.name, set.mods, snapshotName, async () => set);
  }

  /**
   * After a set was replaced, offer to put its previous mods back.
   * @param prev the set as it was
   * @param written the mods it was replaced with; Undo refuses if it changed again since
   */
  function offerReplaceUndo(prev: { id: number; name: string; mods: string[] }, written: string[]) {
    toast({
      message: `Saved your current mods to ${prev.name}.`,
      action: {
        label: "Undo",
        run: async () => {
          const now = sets.find((l) => l.id === prev.id);
          if (!now || !sameMods(now.mods, written)) {
            toast({ message: `Can't undo: ${prev.name} changed since.`, tone: "warn" });
            return;
          }
          if (deps.editRefused()) return;
          try {
            await saveLoadout(prev.id, prev.name, prev.mods);
            await reload();
          } catch (e) {
            reportError(e, "Undoing the change to the set");
          }
        },
      },
    });
  }

  async function saveCurrent() {
    if (deps.editRefused()) return;
    const userSets = () => sets.filter((l) => l.kind !== "auto");
    const name = await askText({
      title: "Save current mods as a set",
      label: "Name",
      confirm: "Save",
      validate: (v) =>
        loadoutNameCheck(
          v,
          userSets().map((l) => l.name),
        ),
    });
    if (name === null || !name.trim()) return;
    if (deps.editRefused()) return;
    // A duplicate name was accepted with "Replace": overwrite that set, don't add a second.
    const existing = findLoadoutByName(name, userSets());
    const current = [...deps.active()];
    try {
      if (existing) {
        const prev = { id: existing.id, name: existing.name, mods: [...existing.mods] };
        await saveLoadout(existing.id, existing.name, current);
        setLast(savedAs(existing));
        await reload();
        offerReplaceUndo(prev, current);
      } else {
        const id = await saveLoadout(null, name.trim(), current);
        setLast(savedAs({ id, name: name.trim() }));
        await reload();
        toast({ message: `Saved your ${modsCount(current.length)} as ${name.trim()}.` });
      }
    } catch (e) {
      reportError(e, "Saving the set");
    }
  }

  async function updateFromCurrent(set: Loadout) {
    // Automatic snapshots are pruned after five more switches: keep edits in a set of your own.
    if (set.kind === "auto") {
      deps.refuse(`${set.name} is an automatic set. Save your current mods as a new set instead.`);
      return;
    }
    if (deps.editRefused()) return;
    const current = [...deps.active()];
    const d = activeSetDiff(set.mods, current);
    const added = d.on.length === 0 ? "None" : formatCount(d.on.length);
    const removed = d.off.length === 0 ? "none" : formatCount(d.off.length);
    if (
      !(await ask({
        title: `Replace ${set.name} with your current ${modsCount(current.length)}?`,
        body:
          `It had ${formatCount(set.mods.length)}. ${added} ${plural(d.on.length, { one: "is", other: "are" })} added and ${removed} removed. ` +
          "Your active mods aren't changed.",
        confirm: "Replace",
      }))
    )
      return;
    if (deps.editRefused()) return;
    const prev = { id: set.id, name: set.name, mods: [...set.mods] };
    try {
      await saveLoadout(set.id, set.name, current);
      setLast(savedAs(set));
      await reload();
      offerReplaceUndo(prev, current);
    } catch (e) {
      reportError(e, "Saving the set");
    }
  }

  async function remove(set: Loadout) {
    if (deps.editRefused()) return;
    if (
      !(await ask({
        title: `Delete ${set.name}?`,
        body: "Your mods aren't affected.",
        confirm: "Delete",
        tone: "danger",
      }))
    )
      return;
    if (deps.editRefused()) return;
    // Kept for Undo: saving it again recreates it with the same name, kind and mods (new id).
    const copy = { name: set.name, mods: [...set.mods], kind: set.kind };
    const wasLast = last?.id === set.id ? last : null;
    try {
      await deleteLoadout(set.id);
      if (wasLast) setLast(null);
      await reload();
      toast({
        message: `Deleted ${copy.name}.`,
        action: {
          label: "Undo",
          run: async () => {
            if (deps.editRefused()) return;
            try {
              const id =
                copy.kind === "auto"
                  ? await saveAutoSnapshot(copy.name, copy.mods)
                  : await saveLoadout(null, copy.name, copy.mods);
              if (wasLast && last === null) setLast({ ...wasLast, id });
              await reload();
            } catch (e) {
              reportError(e, "Restoring the set");
            }
          },
        },
      });
    } catch (e) {
      reportError(e, "Deleting the set");
    }
  }

  async function useSave(s: Savegame) {
    // Which of the save's mods are in the library, and the diff, are read from the last scan:
    // not while a scan may replace it.
    if (!(await deps.settle())) return;
    const library = deps.library();
    if (!library) {
      deps.refuse("Wait for Silo to finish reading your library.");
      return;
    }
    const { present, missing } = saveModStats(s, library);
    if (present.length === 0) {
      deps.refuse(`None of ${s.name}'s mods are in your library yet.`);
      return;
    }
    if (deps.editRefused()) return;
    const target = present.map((m) => m.modName);
    // Reuse a set of the same name only when it already holds exactly these mods.
    const reuse = () =>
      sets.find((l) => l.kind !== "auto" && l.name.trim() === s.name.trim() && sameMods(l.mods, target));
    const resolve = async () => {
      const hit = reuse();
      if (hit) return hit;
      const id = await saveLoadout(null, s.name, target);
      return { id, name: s.name, kind: "user" as const };
    };
    const diff = activeSetDiff([...deps.active()], target, library);
    if (diff.on.length + diff.off.length === 0) {
      try {
        const t = await resolve();
        setLast(savedAs(t));
        await reload();
        toast({ message: `You're already using ${s.name}'s mods. Saved them as a set.` });
      } catch (e) {
        reportError(e, `Saving ${s.name}'s mods as a set`);
      }
      return;
    }
    const snapshotName = `Before ${s.name}`;
    // The missing mods are said once, in `note` (they aren't saved into the set, so the summary's
    // "stays off until you get it" would be wrong here).
    const sum = switchSummary(diff, deps.title, 0, deps.active().size ? snapshotName : null);
    const note =
      missing.length > 0
        ? plural(missing.length, {
            one: " 1 mod the save used isn't in your library and is left out.",
            other: ` ${modsCount(missing.length)} the save used aren't in your library and are left out.`,
          })
        : "";
    if (
      !(await ask({
        title: `Use ${s.name}'s mods?`,
        body: `Silo saves ${modsCount(present.length)} from this save as a set called ${s.name} and switches to it.${note} ${sum.body}`,
        facts: sum.facts,
        list: sum.list,
        confirm: "Switch",
      }))
    )
      return;
    await perform(s.name, target, snapshotName, resolve);
  }

  async function saveShared(name: string, mods: string[]): Promise<number | null> {
    if (deps.editRefused()) return null;
    try {
      const id = await saveLoadout(null, name.trim() || "Shared set", mods);
      await reload();
      return id;
    } catch (e) {
      reportError(e, "Saving the shared set");
      return null;
    }
  }

  async function applyMembership(techNames: string[], edits: MembershipEdit[]): Promise<boolean> {
    if (edits.length === 0) return true;
    if (deps.editRefused()) return false;
    if (!(await deps.settle())) return false;
    const st = currentState();
    const change = activeChange(edits, st.kind === "none" ? null : st.id, deps.active());
    const previous = [...deps.active()];
    // null: the active mods didn't need to change; else whether that switch was clean.
    let switched: boolean | null = null;
    if (change.on.length + change.off.length > 0) {
      const target = new Set(previous);
      for (const t of change.on) target.add(t);
      for (const t of change.off) target.delete(t);
      deps.undo.dismiss();
      const label = change.on.length > 0 ? "Turning on mods" : "Parking mods";
      switched = await deps.setActiveSetTo([...target], label);
      if (switched === null) return false; // refused, cancelled or failed: nothing changed
    }
    // What the switch left, kept now for Undo's staleness check: a scan landing during the save
    // below could bring changes made since.
    const expected = [...deps.active()];
    const before = edits.map((e) => ({ id: e.set.id, name: e.set.name, mods: [...e.set.mods] }));
    const written = new Map(edits.map((e) => [e.set.id, e.mods]));
    try {
      await replaceLoadoutMods(edits.map((e) => ({ id: e.set.id, mods: e.mods })));
    } catch (e) {
      reportError(e, "Saving your mod sets");
      await reload();
      // The mods were switched for a save that didn't happen: offer to put them back.
      if (switched !== null)
        deps.undo.offer({
          message: "Your mod sets weren't saved, but your active mods changed.",
          tone: "warn",
          previous,
          expected,
        });
      return false;
    }
    await reload();
    const names = techNames.map(deps.title);
    let message = membershipMessage(names, edits);
    if (switched !== null) {
      const changed = [...change.on, ...change.off].map(deps.title);
      message += ` ${switchMessage(changed, change.on.length > 0, switched)}`;
    }
    const removed = [...new Set(edits.flatMap((e) => e.removed))];
    const offer = () => {
      const stillOn = removed.filter((t) => deps.active().has(t));
      toast({
        message,
        tone: switched === false ? "warn" : "neutral",
        secondaryAction:
          stillOn.length > 0
            ? { label: stillOn.length === 1 ? "Park it too" : "Park them too", run: () => void deps.park(removed.filter((t) => deps.active().has(t))) }
            : undefined,
        action: { label: "Undo", run: undo },
      });
    };
    const undo = async () => {
      // A running scan first (it may change the active mods, and saving sets can go on meanwhile):
      // every check below must still hold when the write starts.
      if (switched !== null && !(await deps.settle())) return offer();
      const stale = before.find((b) => {
        const now = sets.find((l) => l.id === b.id);
        return !now || !sameMods(now.mods, written.get(b.id) ?? []);
      });
      if (stale) {
        toast({ message: `Can't undo: ${stale.name} changed since.`, tone: "warn" });
        return;
      }
      if (deps.editRefused()) return;
      if (switched !== null) {
        const why = deps.activeUndoBlocked(expected);
        if (why) {
          toast({ message: why, tone: "warn" });
          return;
        }
        // Refused (Silo is busy, and said so): offer the same Undo again.
        if ((await deps.setActiveSetTo(previous, "Switching back")) === null) return offer();
      }
      try {
        await replaceLoadoutMods(before.map((b) => ({ id: b.id, mods: b.mods })));
        await reload();
      } catch (e) {
        reportError(e, "Undoing the change to your mod sets");
      }
    };
    offer();
    return true;
  }

  return {
    get sets() {
      return sets;
    },
    get lastApplied() {
      return last;
    },
    get state() {
      return currentState();
    },
    get current() {
      const s = currentState();
      return s.kind === "none" ? undefined : sets.find((l) => l.id === s.id);
    },
    load,
    switchTo,
    saveCurrent,
    updateFromCurrent,
    remove,
    useSave,
    saveShared,
    applyMembership,
  };
}
