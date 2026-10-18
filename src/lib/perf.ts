// User Timing marks for the §5.5 performance targets (#258). They show up in the DevTools
// Performance panel's Timings track, and the latest of each is readable from the console:
//   performance.getEntriesByName("silo:library-filter")
// Each name keeps only its newest measure, so a long session doesn't pile up entries.
//   silo:scan-to-render   a scan starting → the Library painted with its result
//   silo:library-filter   category, filters and search over the library (each keystroke)
//   silo:library-sort     sorting what's left

/** `performance`, or null where User Timing isn't available. */
function perf(): Performance | null {
  const p = globalThis.performance;
  return p && typeof p.mark === "function" && typeof p.measure === "function" ? p : null;
}

/** Marks the start of `name` (finished by measureFrom). */
export function markStart(name: string): void {
  perf()?.mark(`${name}:start`);
}

/**
 * Measures from markStart(name) to now, replacing the previous measure of that name.
 * @returns the duration in ms, or null when there was no start mark
 */
export function measureFrom(name: string): number | null {
  const p = perf();
  if (!p) return null;
  const start = `${name}:start`;
  if (p.getEntriesByName(start, "mark").length === 0) return null;
  p.clearMeasures(name);
  const m = p.measure(name, start);
  p.clearMarks(start);
  return m?.duration ?? null;
}

/**
 * Runs `fn` between a start mark and a measure called `name`.
 * @returns what `fn` returns
 */
export function timed<T>(name: string, fn: () => T): T {
  if (!perf()) return fn();
  markStart(name);
  try {
    return fn();
  } finally {
    measureFrom(name);
  }
}
