// Badge rows show at most a few badges, then "+N more".

/**
 * Split `items` into the ones to show and a count of the rest.
 * @param items all badges, most important first
 * @param max how many to show (default 3; values below 0 count as 0)
 * @returns `shown` (the first `max`) and `more` (how many were cut)
 */
export function capBadges<T>(items: T[], max = 3): { shown: T[]; more: number } {
  const n = Math.max(0, Math.floor(max));
  return { shown: items.slice(0, n), more: Math.max(0, items.length - n) };
}
