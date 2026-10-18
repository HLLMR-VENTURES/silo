<script lang="ts" module>
  /** A "show only" facet: maps, script mods, uniqueType, the active set; "" for none. */
  export type StatFilter = "" | "maps" | "scripts" | "unique" | "active";
</script>

<script lang="ts">
  // Slim toolbar above the list: the "activate everything in view" master switch, the "show only"
  // facets (maps, script mods, uniqueType, active: one at a time, click again to clear; they were
  // the stat bar's counts), the active tag chip, and the shown count. Category + sort live in
  // the shared FilterBar.
  import Icon from "../../ui/Icon.svelte";
  import { tooltip } from "../../ui/tooltip";
  let {
    selectedTag,
    shownCount,
    allActive,
    activeInFilter,
    disabled,
    disabledReason = "",
    libView = $bindable(),
    statFilter = $bindable(""),
    statCounts,
    onSelectAll,
    onClearTag,
  }: {
    selectedTag: string | null;
    shownCount: number;
    allActive: boolean;
    activeInFilter: number;
    disabled: boolean;
    /** Shown as the switch's tooltip while it's off for a reason ("Silo is moving mods."). */
    disabledReason?: string;
    libView: "tiles" | "rows";
    /** The one "show only" facet in effect, or "". */
    statFilter?: StatFilter;
    statCounts: Record<Exclude<StatFilter, "">, number>;
    onSelectAll: (checked: boolean) => void;
    onClearTag: () => void;
  } = $props();

  const FACETS: { id: Exclude<StatFilter, "">; label: string; title: string }[] = [
    { id: "maps", label: "maps", title: "Filter to maps only" },
    { id: "scripts", label: "script mods", title: "Filter to script mods only" },
    { id: "unique", label: "uniqueType", title: "Filter to mods that declare a uniqueType" },
    { id: "active", label: "active", title: "Filter to the active set only" },
  ];

  let selectAllEl = $state<HTMLInputElement>();
  $effect(() => {
    if (selectAllEl) {
      selectAllEl.indeterminate = activeInFilter > 0 && activeInFilter < shownCount;
    }
  });
</script>

<div class="crumb">
  <label class="sel-all" title={disabledReason || "Activate / deactivate everything in this view"}>
    <input
      type="checkbox"
      class="select-all"
      bind:this={selectAllEl}
      checked={allActive}
      {disabled}
      onchange={(e) => onSelectAll(e.currentTarget.checked)}
    />
    Active
  </label>
  <div class="facets" role="group" aria-label="Show only">
    {#each FACETS as f (f.id)}
      <button
        class="facet"
        class:on={statFilter === f.id}
        aria-pressed={statFilter === f.id}
        title={f.title}
        onclick={() => (statFilter = statFilter === f.id ? "" : f.id)}
      >
        <span class="tnum">{statCounts[f.id]}</span>
        {f.label}
      </button>
    {/each}
  </div>
  {#if selectedTag}
    <button class="crumb-tag" onclick={onClearTag} title="Clear tag filter">
      #{selectedTag} <Icon name="close" size={12} />
    </button>
  {/if}
  <span class="tb-spacer"></span>
  <span class="crumb-count tnum">{shownCount} shown</span>
  <div class="viewtog" role="group" aria-label="View">
    <button
      class="vt"
      class:on={libView === "tiles"}
      use:tooltip={"Tile view"}
      aria-label="Tile view"
      aria-pressed={libView === "tiles"}
      onclick={() => (libView = "tiles")}
    >
      <Icon name="grid" size={14} />
    </button>
    <button
      class="vt"
      class:on={libView === "rows"}
      use:tooltip={"List view"}
      aria-label="List view"
      aria-pressed={libView === "rows"}
      onclick={() => (libView = "rows")}
    >
      <Icon name="list" size={14} />
    </button>
  </div>
</div>

<style>
  .crumb {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 2px 8px;
  }
  .sel-all {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-muted);
    cursor: pointer;
  }
  .crumb-count {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .crumb-tag {
    border: 1px solid color-mix(in srgb, var(--info) 40%, var(--border));
    background: color-mix(in srgb, var(--info) 12%, transparent);
    color: var(--info);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
  }
  .select-all {
    flex: 0 0 auto;
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--primary);
    cursor: pointer;
  }
  .facets {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .facet {
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    padding: 3px 8px;
    border-radius: 999px;
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
  }
  .facet:hover {
    color: var(--text);
  }
  .facet.on {
    background: color-mix(in srgb, var(--primary) 14%, transparent);
    border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
    color: var(--primary-text);
  }
  .tb-spacer {
    flex: 1 1 auto;
  }
  .viewtog {
    display: flex;
    gap: 2px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 2px;
  }
  .vt {
    border: none;
    background: transparent;
    color: var(--text-muted);
    width: 26px;
    height: 22px;
    border-radius: 5px;
    display: inline-grid;
    place-items: center;
    cursor: pointer;
  }
  .vt:hover {
    color: var(--text);
  }
  .vt.on {
    background: color-mix(in srgb, var(--primary) 16%, transparent);
    color: var(--primary-text);
  }
</style>
