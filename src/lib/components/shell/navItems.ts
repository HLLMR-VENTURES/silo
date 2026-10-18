// The rail's destinations, and the accessible name of a rail item with its badge.
import type { IconName } from "../../ui/icons";
import type { ViewId } from "../../state/nav.svelte";

export interface NavItem {
  id: ViewId;
  label: string;
  icon: IconName;
  /** "main" sits at the top; "bottom" is the Tools / Settings / Help group. */
  group: "main" | "bottom";
}

export const NAV_ITEMS: readonly NavItem[] = [
  { id: "library", label: "Library", icon: "library", group: "main" },
  { id: "browse", label: "Browse", icon: "browse", group: "main" },
  { id: "modSets", label: "Mod sets", icon: "modSets", group: "main" },
  { id: "diagnose", label: "Diagnose", icon: "diagnose", group: "main" },
  { id: "updates", label: "Updates", icon: "update", group: "main" },
  { id: "tools", label: "Tools", icon: "tools", group: "bottom" },
  { id: "settings", label: "Settings", icon: "settings", group: "bottom" },
  { id: "help", label: "Help", icon: "help", group: "bottom" },
];

/** A badge worth showing: a positive count or a non-empty word. */
export function hasBadge(badge: number | string | undefined): badge is number | string {
  return typeof badge === "number" ? badge > 0 : typeof badge === "string" && badge.trim() !== "";
}

/**
 * The accessible name of a rail item: "Updates, 4 available", "Diagnose, 2", or just the label.
 * @param item the rail item
 * @param badge its badge, if any
 */
export function navItemName(item: Pick<NavItem, "id" | "label">, badge?: number | string): string {
  if (!hasBadge(badge)) return item.label;
  if (item.id === "updates" && typeof badge === "number") return `${item.label}, ${badge} available`;
  return `${item.label}, ${badge}`;
}
