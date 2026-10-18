<script lang="ts">
  // Settings: General, Mods folder, Library, Accounts, Diagnostics, Maintenance, About. Snippets
  // App renders (each a section component from components/settings/); the tab lives in
  // nav.tab.settings, like Tools.
  //   <SettingsView>{#snippet generalTab()}<GeneralSettings ... />{/snippet}...</SettingsView>
  // go("settings", "accounts") (Browse's "connect GitHub" link) opens Accounts and scrolls to and
  // focuses the GitHub card, once per request; a tab click or a plain return to Settings doesn't.
  import { tick, type Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import { SETTINGS_FOCUS_TARGETS, SETTINGS_TABS } from "../components/settings/sections";
  import { nav, pendingTabRequest, takeTabRequest, type SettingsTab } from "../state/nav.svelte";

  interface Props {
    generalTab: Snippet;
    modsFolderTab: Snippet;
    libraryTab: Snippet;
    accountsTab: Snippet;
    diagnosticsTab: Snippet;
    maintenanceTab: Snippet;
    aboutTab: Snippet;
  }

  let {
    generalTab,
    modsFolderTab,
    libraryTab,
    accountsTab,
    diagnosticsTab,
    maintenanceTab,
    aboutTab,
  }: Props = $props();

  const panels: Record<SettingsTab, Snippet> = $derived({
    general: generalTab,
    modsFolder: modsFolderTab,
    library: libraryTab,
    accounts: accountsTab,
    diagnostics: diagnosticsTab,
    maintenance: maintenanceTab,
    about: aboutTab,
  });

  $effect(() => {
    const requested = pendingTabRequest("settings");
    if (requested === undefined) return;
    void tick().then(() => {
      takeTabRequest("settings");
      const id = SETTINGS_FOCUS_TARGETS[requested as SettingsTab];
      const el = id ? document.getElementById(id) : null;
      if (!el) return;
      el.scrollIntoView?.({ block: "start" });
      el.focus({ preventScroll: true });
    });
  });
</script>

<ViewBoundary name="Settings">
  <ViewHeader title="Settings" tabs={SETTINGS_TABS} bind:tab={nav.tab.settings}>
    {#snippet panel(id)}
      <div class="tab-body">{@render panels[id as SettingsTab]?.()}</div>
    {/snippet}
  </ViewHeader>
</ViewBoundary>

<style>
  /* The tab's cards, stacked and left-aligned under the tabs (Tools lays out its panels the same way). */
  .tab-body {
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
    max-width: 55rem;
    padding-top: var(--sp-4);
  }
</style>
