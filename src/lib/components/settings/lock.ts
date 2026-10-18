// Why Settings' folder- and library-changing controls are off, as one value the tabs share:
//   const lock = $derived(settingsLock({ lockReason, scanning, moveUnfinished }));
//   <Button disabled={lock.locked} title={lock.reason}>Change…</Button>
//   {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}

export const MOVE_UNFINISHED_REASON = "Finish or cancel the archive move first";
export const SCANNING_REASON = "Wait for the scan to finish.";

export interface SettingsLock {
  /** The control is off. */
  locked: boolean;
  /** Why it's off (the control's title and the helper text under it); undefined when it's on. */
  reason: string | undefined;
}

/**
 * Folder- and library-changing controls are off during a library write, a scan, or mid-move.
 * @param s lockReason: the running library write's reason, or ""; scanning: a scan runs;
 *   moveUnfinished: an archive move is running or paused
 * @return whether they're off and why
 */
export function settingsLock(s: {
  lockReason: string;
  scanning: boolean;
  moveUnfinished: boolean;
}): SettingsLock {
  const locked = !!s.lockReason || s.scanning || s.moveUnfinished;
  const reason = s.moveUnfinished
    ? MOVE_UNFINISHED_REASON
    : s.lockReason || (s.scanning ? SCANNING_REASON : undefined);
  return { locked, reason: locked ? reason : undefined };
}

/**
 * Re-read every mod is off only during a library write or a scan (a paused move doesn't stop it).
 * Its title stays the write's reason alone, as before; `reason` also names a running scan so the
 * helper text under it is never empty.
 * @param s lockReason: the running library write's reason, or ""; scanning: a scan runs
 */
export function rereadLock(s: { lockReason: string; scanning: boolean }): SettingsLock & {
  title: string | undefined;
} {
  const locked = !!s.lockReason || s.scanning;
  return {
    locked,
    reason: s.lockReason || (s.scanning ? SCANNING_REASON : undefined),
    title: s.lockReason || undefined,
  };
}
