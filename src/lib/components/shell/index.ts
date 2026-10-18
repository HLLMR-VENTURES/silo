// App shell barrel: `import { AppShell, NavRail } from "./lib/components/shell"`.
export { default as AppHeader } from "./AppHeader.svelte";
export { default as AppShell } from "./AppShell.svelte";
export { default as NavRail } from "./NavRail.svelte";
export { default as PlayButton } from "./PlayButton.svelte";
export { default as SearchField } from "./SearchField.svelte";
export { default as StatusChip } from "./StatusChip.svelte";
export { default as StatusStrip } from "./StatusStrip.svelte";
export { default as ViewHeader } from "./ViewHeader.svelte";

export type { PlayMenuItem } from "./PlayButton.svelte";
export { MAX_STRIP_ITEMS, type StatusStripItem } from "./StatusStrip.svelte";
export { hasBadge, NAV_ITEMS, navItemName, type NavItem } from "./navItems";
export { nextRegion, presentRegions, regionOf, REGIONS, type Region } from "./regions";
export { statusChipModel, type StatusChipModel, type StatusTone } from "./status";
