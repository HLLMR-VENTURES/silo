// Shared prop types for the overlay primitives (Popover, Menu, MenuButton).
import type { IconName } from "./icons";

/** Where a Popover opens relative to its anchor; it flips when that would overflow the viewport. */
export type Placement = "bottom-start" | "bottom-end" | "top-start" | "top-end";

/** Why a Popover closed itself. */
export type PopoverCloseReason = "escape" | "outside" | "blur";

/** One entry in a Menu. */
export interface MenuItem {
  id: string;
  label: string;
  icon?: IconName;
  /** Destructive action: shown in the danger color. */
  danger?: boolean;
  disabled?: boolean;
  /** Draw a separator line above this item. */
  separatorBefore?: boolean;
  /** Display-only shortcut hint, e.g. "Ctrl+D". */
  shortcut?: string;
}
