<script lang="ts">
  // The filters in effect, under the Library toolbar, each a removable chip
  // ("Remove filter: Maps"), then Clear filters. Nothing renders when no filter is on.
  //   <ActiveFilters chips={filterChips(f, cat, sub)} onremove={(id) => ...} onclear={clear}
  //     onemptied={() => toolbar?.focusFilters()} />
  // When the last chip goes (its ✕, or Clear filters) while focus was on it, `onemptied` says
  // where focus goes instead (the Filters button), so it isn't lost to the page.
  import Chip from "../../ui/Chip.svelte";
  import Button from "../../ui/Button.svelte";
  import type { FilterChip } from "./filters";
  import { returnFocusWhenHidden } from "./focusReturn.svelte";

  interface Props {
    chips: FilterChip[];
    onremove: (id: string) => void;
    onclear: () => void;
    /** Focus moves here when the chips go away while one of them had it. */
    onemptied?: () => void;
  }

  let { chips, onremove, onclear, onemptied }: Props = $props();

  let root = $state<HTMLElement | null>(null);
  returnFocusWhenHidden(
    () => chips.length > 0,
    () => root,
    () => onemptied?.(),
  );
</script>

{#if chips.length > 0}
  <div class="active-filters" role="group" bind:this={root} aria-label="Filters in effect">
    {#each chips as c (c.id)}
      <Chip label={c.label} onremove={() => onremove(c.id)} />
    {/each}
    <Button size="sm" variant="link" onclick={onclear}>Clear filters</Button>
  </div>
{/if}

<style>
  .active-filters {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0 0 8px;
  }
</style>
