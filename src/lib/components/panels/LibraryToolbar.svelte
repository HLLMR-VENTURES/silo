<script lang="ts">
  // Slim toolbar above the list: the "activate everything in view" master switch, the active
  // tag chip, and the shown count. Category + sort now live in the shared FilterBar.
  import Icon from "../../ui/Icon.svelte";
  let {
    selectedTag,
    shownCount,
    allActive,
    activeInFilter,
    disabled,
    libView = $bindable(),
    onSelectAll,
    onClearTag,
  }: {
    selectedTag: string | null;
    shownCount: number;
    allActive: boolean;
    activeInFilter: number;
    disabled: boolean;
    libView: "tiles" | "rows";
    onSelectAll: (checked: boolean) => void;
    onClearTag: () => void;
  } = $props();

  let selectAllEl = $state<HTMLInputElement>();
  $effect(() => {
    if (selectAllEl) {
      selectAllEl.indeterminate = activeInFilter > 0 && activeInFilter < shownCount;
    }
  });
</script>

<div class="crumb">
  <label class="sel-all" title="Activate / deactivate everything in this view">
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
      title="Tile view"
      aria-label="Tile view"
      aria-pressed={libView === "tiles"}
      onclick={() => (libView = "tiles")}
    >
      <Icon name="grid" size={14} />
    </button>
    <button
      class="vt"
      class:on={libView === "rows"}
      title="List view"
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
