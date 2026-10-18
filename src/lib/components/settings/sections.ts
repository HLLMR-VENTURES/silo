// The Settings tabs, in order, and the element a deep link to a tab focuses.
//   go("settings", "accounts") // opens Accounts and focuses #settings-accounts
import type { SettingsTab } from "../../state/nav.svelte";
import type { TabItem } from "../../ui/types";

export const SETTINGS_TABS: TabItem<SettingsTab>[] = [
  { id: "general", label: "General" },
  { id: "modsFolder", label: "Mods folder" },
  { id: "library", label: "Library" },
  { id: "accounts", label: "Accounts" },
  { id: "diagnostics", label: "Diagnostics" },
  { id: "maintenance", label: "Maintenance" },
  { id: "about", label: "About" },
];

/** The GitHub account card (AccountSettings). */
export const ACCOUNTS_SECTION_ID = "settings-accounts";

/** The element go("settings", tab) scrolls to and focuses, for tabs that have one. */
export const SETTINGS_FOCUS_TARGETS: Partial<Record<SettingsTab, string>> = {
  accounts: ACCOUNTS_SECTION_ID,
};
