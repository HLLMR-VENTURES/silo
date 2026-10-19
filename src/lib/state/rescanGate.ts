// Whether a library scan may run now, and the one scan to run later when it can't (#228).
//   const gate = rescanGate((auto) => void runScan(auto));
//   if (!gate.admit(auto, heldByCaller)) return; // queued: runs once the write class frees
// Only a foreground library write holds a scan back; a background op (the archive move) never
// does. Skipped scans collapse into one pending rescan (a flag, not a queue), so startup, a
// folder switch or a status refresh never leaves the library empty behind a write.

import { blockedForeground, onIdle } from "./ops.svelte";

export interface RescanGate {
  /**
   * @param auto the scan would allow auto-filing afterwards (kept for the pending one)
   * @param heldByCaller the caller itself holds the write class for this scan
   * @returns true to scan now; false when a rescan was queued instead
   */
  admit(auto: boolean, heldByCaller?: boolean): boolean;
  /** A rescan is waiting for the write class to free. */
  readonly pending: boolean;
}

/**
 * @param run starts the pending rescan, with the `auto` of any scan it stands for
 */
export function rescanGate(run: (auto: boolean) => void): RescanGate {
  let pending = false;
  let pendingAuto = false;
  return {
    admit(auto, heldByCaller = false) {
      if (heldByCaller || !blockedForeground("library-write").blocked) return true;
      pendingAuto ||= auto;
      if (!pending) {
        pending = true;
        onIdle("library-write", () => {
          pending = false;
          const a = pendingAuto;
          pendingAuto = false;
          run(a);
        });
      }
      return false;
    },
    get pending() {
      return pending;
    },
  };
}

/** The focus rescan waits this long after any scan, so alt-tabbing doesn't thrash. */
export const FOCUS_RESCAN_AFTER_MS = 4000;

/**
 * Whether coming back to the window may rescan the library. Not while a scan, a library write,
 * a running archive move or a diagnosis runs; not while the mods folder is being switched (its
 * folder picker and dialogs take focus, and a scan started then reads the old folder); and not
 * within FOCUS_RESCAN_AFTER_MS of the last scan.
 */
export function focusRescanAllowed(s: {
  scanning: boolean;
  writeBlocked: boolean;
  moveRunning: boolean;
  bisecting: boolean;
  switchingFolder: boolean;
  hasRoots: boolean;
  sinceLastScanMs: number;
}): boolean {
  return (
    !s.scanning &&
    !s.writeBlocked &&
    !s.moveRunning &&
    !s.bisecting &&
    !s.switchingFolder &&
    s.hasRoots &&
    s.sinceLastScanMs > FOCUS_RESCAN_AFTER_MS
  );
}

/**
 * Resolve once `busy()` is false (checked every `stepMs`), e.g. a scan of the old mods folder
 * that was already running when the folder changed.
 */
export async function waitWhile(busy: () => boolean, stepMs = 50): Promise<void> {
  while (busy()) await new Promise((r) => setTimeout(r, stepMs));
}

/** Waiting for a running scan to end, briefly (a switch made right after alt-tabbing back). */
export interface ScanSettler {
  /**
   * @param busy a scan is running now
   * @param ms how long to wait at most
   * @returns true once no scan runs (at once when none does), false when it still ran after `ms`
   */
  wait(busy: boolean, ms: number): Promise<boolean>;
  /** The scan ended: everyone waiting goes ahead. */
  ended(): void;
}

/** One per scan owner (App): `ended()` from an effect that sees `scanning` go false. */
export function createScanSettler(): ScanSettler {
  let waiters: ((ok: boolean) => void)[] = [];
  return {
    wait(busy, ms) {
      if (!busy) return Promise.resolve(true);
      return new Promise((resolve) => {
        const finish = (ok: boolean) => {
          clearTimeout(timer);
          waiters = waiters.filter((w) => w !== finish);
          resolve(ok);
        };
        const timer = setTimeout(() => finish(false), ms);
        waiters.push(finish);
      });
    },
    ended() {
      const now = waiters;
      waiters = [];
      for (const w of now) w(true);
    },
  };
}
