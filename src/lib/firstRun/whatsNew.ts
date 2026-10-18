// The "What's new in Silo 1.0" items (WhatsNew.svelte).
import type { IconName } from "../ui/icons";

export const WHATS_NEW: { icon: IconName; title: string; body: string }[] = [
  {
    icon: "menu",
    title: "A new layout",
    body: "Library, Browse, Mod sets, Diagnose and Updates sit in the sidebar. Settings and Help are at its bottom, and Play is in the header.",
  },
  {
    icon: "modSets",
    title: "Mod sets",
    body: "Loadouts are now mod sets. Save your active mods as a set, switch between sets, and share one with friends for multiplayer.",
  },
  {
    icon: "undo",
    title: "Undo",
    body: "Switching a mod on or off shows Undo for a few seconds, and Undo organizing in Settings → Maintenance puts every mod back.",
  },
  {
    icon: "controls",
    title: "Keyboard shortcuts",
    body: "Ctrl+K searches, F5 rescans, Alt+1 to Alt+5 switch pages. Press ? anywhere for the full list.",
  },
  {
    icon: "info",
    title: "Plain-English errors",
    body: "When something goes wrong, Silo says what happened and what to do, with Copy details for a bug report.",
  },
];
