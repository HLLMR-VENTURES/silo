<script lang="ts">
  // Takes the toolbar's place while any mod is selected:
  //   12 selected · [Turn on] [Park] [Favorite] [Hide] · [Select all 140 shown] · Clear
  //   <BulkBar count={sel.size} shownCount={visible.length} onturnon={...} onpark={...}
  //     onfavorite={...} onhide={...} onselectall={...} onclear={() => sel.clear()} />
  // Turn on / Park are off (with the reason as the tooltip) while another library change runs.
  // Favorite / Hide flip to Remove favorite / Unhide when every selected mod already is one.
  import Button from "../../ui/Button.svelte";
  import { tooltip } from "../../ui/tooltip";

  interface Props {
    count: number;
    shownCount: number;
    /** Every shown mod is selected (Select all has nothing to add). */
    allShownSelected?: boolean;
    /** Every selected mod is a favorite: the button removes the favorite instead. */
    allFavorite?: boolean;
    /** Every selected mod is hidden: the button unhides instead. */
    allHidden?: boolean;
    /** Why Turn on / Park can't run now ("Silo is moving mods."), or "". */
    writeReason?: string;
    onturnon: () => void;
    onpark: () => void;
    onfavorite: (on: boolean) => void;
    onhide: (on: boolean) => void;
    onselectall: () => void;
    onclear: () => void;
  }

  let {
    count,
    shownCount,
    allShownSelected = false,
    allFavorite = false,
    allHidden = false,
    writeReason = "",
    onturnon,
    onpark,
    onfavorite,
    onhide,
    onselectall,
    onclear,
  }: Props = $props();
</script>

<div class="bulk" role="toolbar" aria-label="Selected mods">
  <span class="count tnum" aria-live="polite">{count} selected</span>
  <span class="sep" aria-hidden="true">·</span>
  <span class="group" use:tooltip={writeReason || null}>
    <Button size="sm" variant="primary" disabled={!!writeReason} onclick={onturnon}>Turn on</Button>
    <Button size="sm" disabled={!!writeReason} onclick={onpark}>Park</Button>
  </span>
  <Button size="sm" icon="favorite" onclick={() => onfavorite(!allFavorite)}>
    {allFavorite ? "Remove favorite" : "Favorite"}
  </Button>
  <Button size="sm" icon={allHidden ? "visible" : "hidden"} onclick={() => onhide(!allHidden)}>
    {allHidden ? "Unhide" : "Hide"}
  </Button>
  <span class="sep" aria-hidden="true">·</span>
  <Button size="sm" variant="ghost" disabled={allShownSelected} onclick={onselectall}>
    Select all {shownCount} shown
  </Button>
  <span class="sep" aria-hidden="true">·</span>
  <Button size="sm" variant="link" onclick={onclear}>Clear</Button>
</div>

<style>
  .bulk {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    min-height: 2.5rem;
    margin: 0 0 8px;
    padding: 4px 10px;
    border: 1px solid color-mix(in srgb, var(--primary) 40%, var(--border));
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--primary) 8%, var(--surface));
  }
  .count {
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .group {
    display: inline-flex;
    gap: var(--sp-2);
  }
  .sep {
    color: var(--text-muted);
  }
</style>
