// The verdict badges in the mod drawer's header, most important first. Pure, so it unit-tests
// without the app. Same specs and order as the Library rows (rowBadges: problems, then status),
// cut to the verdicts the header carries:
//   Modified (n files) or Verified build, Problem, Update
//   drawerBadges(mod, { verify, problem: true }) -> [Modified (2 files), Problem]
// An unchecked or unverifiable mod gets no integrity badge: no verdict isn't a warning.
import { plural } from "../../plural";
import type { Conflict, LogReport, ModEntry, VerifyResult } from "../../types";
import { rowBadges, type BadgeGroup, type BadgeSpec } from "../library/rowBadges";

/** What the drawer knows about the mod beyond its scan entry. */
export interface DrawerBadgeContext {
  /** The integrity check's result, once the mod was checked this session. */
  verify?: VerifyResult | null;
  hasUpdate?: boolean;
  /** In a critical conflict, or named by the last run's crash (see isProblem). */
  problem?: boolean;
}

type BadgeMod = Parameters<typeof rowBadges>[0];

const GROUP_RANK: Record<BadgeGroup, number> = { problem: 0, status: 1, info: 2 };

/** The number of files that differ from the published build. */
export function modifiedFileCount(r: VerifyResult): number {
  return r.changed.length + r.added.length + r.removed.length;
}

/**
 * The header badges for one mod, in display order. At most three by construction (Verified and
 * Modified exclude each other).
 * @param mod the scanned mod
 * @param ctx the integrity result, update and problem facts
 */
export function drawerBadges(mod: BadgeMod, ctx: DrawerBadgeContext = {}): BadgeSpec[] {
  const out = rowBadges(mod, { verdict: ctx.verify?.status, hasUpdate: ctx.hasUpdate })
    .filter((b) => b.id === "modified" || b.id === "verified" || b.id === "update")
    .map((b): BadgeSpec => {
      if (b.id === "modified" && ctx.verify) {
        const n = modifiedFileCount(ctx.verify);
        if (n > 0) return { ...b, label: `Modified (${n} ${plural(n, { one: "file", other: "files" })})` };
      }
      if (b.id === "update") return { ...b, label: "Update available" };
      return b;
    });
  if (ctx.problem) {
    out.push({
      id: "problem",
      label: "Problem",
      tone: "danger",
      icon: "problem",
      group: "problem",
      title: "In a conflict that needs fixing, or named by the last crash. See below.",
    });
  }
  // Stable: keeps rowBadges' order inside each group.
  return out.sort((a, b) => GROUP_RANK[a.group] - GROUP_RANK[b.group]);
}

/** Does a conflict name this mod? Conflict.mods carries tech names and sometimes titles. */
export function conflictNames(mod: Pick<ModEntry, "techName" | "title">, c: Conflict): boolean {
  return c.mods.includes(mod.techName) || (!!mod.title && c.mods.includes(mod.title));
}

/** The last run's log entry for this mod, if the log named it. */
export function logHealthOf(mod: Pick<ModEntry, "techName">, report: LogReport | null) {
  return report?.mods.find((m) => m.modName === mod.techName) ?? null;
}

/**
 * Is the mod a Problem: in a critical conflict, or logged errors in a last run that crashed?
 * @param mod the mod
 * @param conflicts the active set's conflicts
 * @param report the last game log's triage, if read
 */
export function isProblem(
  mod: Pick<ModEntry, "techName" | "title">,
  conflicts: Conflict[],
  report: LogReport | null,
): boolean {
  if (conflicts.some((c) => c.severity === "critical" && conflictNames(mod, c))) return true;
  const h = logHealthOf(mod, report);
  return !!report?.crashed && !!h && h.errors > 0;
}

/**
 * The other mods this one conflicts with (warning or worse), for "Conflicts with 2 active mods".
 * @param mod the mod
 * @param conflicts the active set's conflicts
 * @returns the other mods' names, each once, in first-seen order
 */
export function conflictPartners(mod: Pick<ModEntry, "techName" | "title">, conflicts: Conflict[]): string[] {
  const self = new Set([mod.techName, mod.title ?? ""]);
  const out = new Set<string>();
  for (const c of conflicts) {
    if (c.severity === "info" || !conflictNames(mod, c)) continue;
    for (const m of c.mods) if (!self.has(m)) out.add(m);
  }
  return [...out];
}
