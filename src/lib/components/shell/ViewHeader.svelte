<script lang="ts">
  // Every view's title block: an <h2> in Fraunces, an optional description and actions, and
  // optional tabs. With `tabs`, the view's body goes in `panel` (the Tabs primitive renders it):
  //   <ViewHeader title="Diagnose" {tabs} bind:tab={nav.tab.diagnose}>
  //     {#snippet actions()}<Button>Run check</Button>{/snippet}
  //     {#snippet panel(id)}...{/snippet}
  //   </ViewHeader>
  import type { Snippet } from "svelte";
  import Tabs from "../../ui/Tabs.svelte";
  import type { TabItem } from "../../ui/types";

  interface Props {
    title: string;
    description?: string;
    actions?: Snippet;
    tabs?: TabItem[];
    /** The selected tab id (bindable). */
    tab?: string;
    tabsLabel?: string;
    ontabchange?: (id: string) => void;
    /** The body under the tabs; required with `tabs`. */
    panel?: Snippet<[string]>;
  }

  let {
    title,
    description,
    actions,
    tabs,
    tab = $bindable(),
    tabsLabel,
    ontabchange,
    panel,
  }: Props = $props();
</script>

<div class="view-header">
  <div class="top">
    <div class="titles">
      <h2 class="title">{title}</h2>
      {#if description}<p class="description">{description}</p>{/if}
    </div>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </div>
</div>
{#if tabs && tabs.length && panel}
  <Tabs {tabs} bind:selected={tab} ariaLabel={tabsLabel ?? title} onchange={ontabchange} {panel} />
{/if}

<style>
  .view-header {
    margin-bottom: var(--sp-4);
  }
  .top {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-4);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  .title {
    font-size: var(--fs-xl);
    line-height: var(--lh-xl);
  }
  .description {
    margin: var(--sp-1) 0 0;
    color: var(--muted);
    max-width: 65ch;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex: none;
  }
</style>
