<script lang="ts">
  // The header bar's contents (AppShell wraps it in <header>): search scoped to the view, then the
  // status chip, the activity bell and Play.
  //   <AppHeader bind:search searchPlaceholder={searchPlaceholderFor(nav.view)} onsearchsubmit={run}>
  //     {#snippet status()}<StatusChip ... />{/snippet}
  //     {#snippet play()}<PlayButton ... />{/snippet}
  //   </AppHeader>
  // The app's only <h1> lives here, visually hidden ("Silo"); each view titles itself with an <h2>
  // (ViewHeader), so headings read Silo > Library > sections.
  import type { Snippet } from "svelte";
  import ActivityButton from "../ActivityButton.svelte";
  import SearchField from "./SearchField.svelte";

  interface Props {
    search?: string;
    searchPlaceholder?: string;
    onsearchsubmit?: (value: string) => void;
    status?: Snippet;
    play?: Snippet;
    /** Replaces the default ActivityButton. */
    activity?: Snippet;
  }

  let {
    search = $bindable(""),
    searchPlaceholder = "Search your mods",
    onsearchsubmit,
    status,
    play,
    activity,
  }: Props = $props();

  let field = $state<SearchField | null>(null);

  /** Focus the search field (Ctrl+K). */
  export function focusSearch(): void {
    field?.focus();
  }
</script>

<div class="app-header">
  <h1 class="vh">Silo</h1>
  <div class="search">
    <SearchField
      bind:this={field}
      bind:value={search}
      placeholder={searchPlaceholder}
      label={searchPlaceholder}
      onsubmit={onsearchsubmit}
    />
  </div>
  <div class="actions">
    {@render status?.()}
    {#if activity}
      {@render activity()}
    {:else}
      <ActivityButton />
    {/if}
    {@render play?.()}
  </div>
</div>

<style>
  .app-header {
    display: flex;
    align-items: center;
    gap: var(--sp-4);
    height: 100%;
    padding: 0 var(--sp-4);
  }
  .search {
    flex: 1 1 auto;
    min-width: 8rem;
    max-width: 32rem;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    margin-left: auto;
    flex: none;
  }
  .vh {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
</style>
