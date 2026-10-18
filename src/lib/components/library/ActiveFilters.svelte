<script lang="ts">
  // The filters in effect, under the Library toolbar, each a removable chip
  // ("Remove filter: Maps"), then Clear filters. Nothing renders when no filter is on.
  //   <ActiveFilters chips={filterChips(f, cat, sub)} onremove={(id) => ...} onclear={clear} />
  import Chip from "../../ui/Chip.svelte";
  import Button from "../../ui/Button.svelte";
  import type { FilterChip } from "./filters";

  interface Props {
    chips: FilterChip[];
    onremove: (id: string) => void;
    onclear: () => void;
  }

  let { chips, onremove, onclear }: Props = $props();
</script>

{#if chips.length > 0}
  <div class="active-filters" role="group" aria-label="Filters in effect">
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
