<script lang="ts">
  // The drawer's primary row: the Active switch, Settings (when Silo can edit them) and a ⋯ menu
  // with the rest of the per-mod actions. Menu ids: favorite, broken, hidden, category, showFile,
  // browse (see drawerMenuItems).
  import Switch from "../../ui/Switch.svelte";
  import Button from "../../ui/Button.svelte";
  import MenuButton from "../../ui/MenuButton.svelte";
  import type { CurationRow } from "../../types";
  import { drawerMenuItems, type DrawerMenuId } from "./drawerMenu";

  interface Props {
    title: string;
    active: boolean;
    /** Settings Silo can edit now. */
    hasSettings: boolean;
    /** The mod has settings, but they aren't written to modSettings/ yet. */
    settingsLater: boolean;
    curation: Pick<CurationRow, "favorite" | "hidden" | "broken">;
    onToggleActive: () => void;
    onOpenSettings: () => void;
    onMenu: (id: DrawerMenuId) => void;
  }

  let { title, active, hasSettings, settingsLater, curation, onToggleActive, onOpenSettings, onMenu }: Props =
    $props();

  const items = $derived(drawerMenuItems(curation));
</script>

<div class="actions">
  <Switch checked={active} controlled label="Active" onchange={onToggleActive} />
  <span class="state">{active ? "The game loads it" : "Parked, kept in your library"}</span>
  <span class="spacer"></span>
  {#if hasSettings}
    <Button size="sm" icon="modSettings" onclick={onOpenSettings}>Settings</Button>
  {/if}
  <MenuButton
    label="More actions for {title}"
    icon="more"
    iconOnly
    size="sm"
    {items}
    placement="bottom-end"
    onselect={(id) => onMenu(id as DrawerMenuId)}
  />
</div>
{#if !hasSettings && settingsLater}
  <p class="hint">This mod has settings. Silo can edit them once the game writes them, usually after you play with the mod once.</p>
{/if}

<style>
  .actions {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-4);
    flex-wrap: wrap;
  }
  .state {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .spacer {
    flex: 1 1 auto;
  }
  .hint {
    margin: var(--sp-2) 0 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.45;
  }
</style>
