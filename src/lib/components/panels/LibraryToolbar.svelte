<script lang="ts">
  // The toolbar above the Library list: the All / Active / Parked segment, the Favorites and
  // Needs attention chips, the Filters popover (Hidden, Flagged, In conflict, Has settings, Needs
  // update, Maps, Script mods, uniqueType, a tag), the shown count and the list / cards toggle.
  // Category and sort live in the shared FilterBar above it; the chips of the filters in effect
  // render under it (ActiveFilters). Presentation only: App owns the filters.
  //   <LibraryToolbar {filters} onchange={(f) => (libFilters = f)} onneedsupdate={toggleNeedsUpdate}
  //     tags={allTags} counts={...} shownCount={visible.length} bind:libView />
  import { tick } from "svelte";
  import Button from "../../ui/Button.svelte";
  import Icon from "../../ui/Icon.svelte";
  import Popover from "../../ui/Popover.svelte";
  import Segmented from "../../ui/Segmented.svelte";
  import Select from "../../ui/Select.svelte";
  import { tooltip } from "../../ui/tooltip";
  import type { ChoiceOption } from "../../ui/types";
  import {
    POPOVER_FLAGS,
    popoverCount,
    type FilterFlag,
    type LibraryFilters,
    type StatusFilter,
  } from "../library/filters";

  interface Props {
    filters: LibraryFilters;
    onchange: (next: LibraryFilters) => void;
    /** "Needs update" runs an update check the first time, so App switches it (maybe not on). */
    onneedsupdate: (on: boolean) => void | Promise<void>;
    /** An update check is running (Needs update waits for it). */
    updateChecking?: boolean;
    /** Every tag in the library; the popover offers a tag only when there are some. */
    tags?: string[];
    /** Counts shown next to a filter, when known. */
    counts?: Partial<Record<FilterFlag, number>>;
    shownCount: number;
    /** List (the default) or cards. */
    libView: "list" | "cards";
  }

  let {
    filters,
    onchange,
    onneedsupdate,
    updateChecking = false,
    tags = [],
    counts = {},
    shownCount,
    libView = $bindable(),
  }: Props = $props();

  const STATUS: ChoiceOption<StatusFilter>[] = [
    { value: "all", label: "All" },
    { value: "active", label: "Active" },
    { value: "parked", label: "Parked" },
  ];

  let filtersOpen = $state(false);
  let trigger = $state<HTMLElement | null>(null);
  let panel = $state<HTMLDivElement | null>(null);
  const inPopover = $derived(popoverCount(filters));
  const tagOptions = $derived<ChoiceOption[]>([
    { value: "", label: "Any tag" },
    ...tags.map((t) => ({ value: t, label: t })),
  ]);

  function set<K extends keyof LibraryFilters>(key: K, value: LibraryFilters[K]) {
    onchange({ ...filters, [key]: value });
  }
  /** The checkboxes are controlled: once App has decided, the box shows what's in effect. */
  async function setFlag(e: Event & { currentTarget: HTMLInputElement }, id: FilterFlag) {
    const box = e.currentTarget;
    if (id === "needsUpdate") await onneedsupdate(box.checked);
    else set(id, box.checked);
    await tick();
    box.checked = filters[id];
  }
  function label(id: FilterFlag, text: string): string {
    if (id === "needsUpdate" && updateChecking) return `${text} (checking…)`;
    const n = counts[id];
    return n ? `${text} (${n})` : text;
  }
  /** Off while it can't narrow anything (no conflicts), unless it's on (so it can be cleared). */
  function flagDisabled(id: FilterFlag): boolean {
    if (filters[id]) return id === "needsUpdate" && updateChecking;
    if (id === "needsUpdate") return updateChecking;
    return id === "conflicted" && counts.conflicted === 0;
  }

  let root = $state<HTMLElement | null>(null);

  /** Focus the Filters button (focus lands here when the last filter chip is removed). */
  export function focusFilters(): boolean {
    const b = trigger?.querySelector<HTMLElement>("button");
    b?.focus();
    return !!b;
  }

  /** Focus the toolbar's first control. */
  export function focusFirst(): boolean {
    const b = root?.querySelector<HTMLElement>("button:not(:disabled), input:not(:disabled), select:not(:disabled)");
    b?.focus();
    return !!b;
  }

  async function toggleFilters() {
    filtersOpen = !filtersOpen;
    if (!filtersOpen) return;
    await tick();
    panel?.querySelector<HTMLElement>("input, select")?.focus();
  }
</script>

<div class="toolbar" bind:this={root}>
  <Segmented
    ariaLabel="Show"
    size="sm"
    options={STATUS}
    value={filters.status}
    onchange={(v) => set("status", v)}
  />
  <div class="chips" role="group" aria-label="Quick filters">
    <button
      type="button"
      class="tchip"
      aria-pressed={filters.favorites}
      onclick={() => set("favorites", !filters.favorites)}
    >
      <Icon name="favorite" size={13} filled={filters.favorites} /> Favorites
    </button>
    <button
      type="button"
      class="tchip"
      aria-pressed={filters.attention}
      use:tooltip={"Mods with a problem (can't load, conflict, modified, flagged) or an update"}
      onclick={() => set("attention", !filters.attention)}
    >
      <Icon name="problem" size={13} />
      Needs attention{counts.attention ? ` (${counts.attention})` : ""}
    </button>
  </div>
  <span class="filters-btn" bind:this={trigger}>
    <Button
      size="sm"
      variant="ghost"
      icon="menu"
      aria-haspopup="dialog"
      aria-expanded={filtersOpen}
      aria-controls={filtersOpen ? "library-filters" : undefined}
      onclick={toggleFilters}
    >
      Filters{inPopover ? ` (${inPopover})` : ""}
    </Button>
  </span>
  <span class="tb-spacer"></span>
  <span class="count tnum">{shownCount} shown</span>
  <div class="viewtog" role="group" aria-label="View">
    <button
      type="button"
      class="vt"
      class:on={libView === "list"}
      use:tooltip={"List view"}
      aria-label="List view"
      aria-pressed={libView === "list"}
      onclick={() => (libView = "list")}
    >
      <Icon name="list" size={14} />
    </button>
    <button
      type="button"
      class="vt"
      class:on={libView === "cards"}
      use:tooltip={"Card view"}
      aria-label="Card view"
      aria-pressed={libView === "cards"}
      onclick={() => (libView = "cards")}
    >
      <Icon name="grid" size={14} />
    </button>
  </div>
</div>

<Popover
  bind:open={filtersOpen}
  bind:ref={panel}
  anchor={trigger}
  placement="bottom-start"
  role="dialog"
  id="library-filters"
  aria-label="Filters"
>
  <div class="panel">
    {#each POPOVER_FLAGS as f (f.id)}
      <label class="opt" class:disabled={flagDisabled(f.id)}>
        <input
          type="checkbox"
          checked={filters[f.id]}
          disabled={flagDisabled(f.id)}
          onchange={(e) => setFlag(e, f.id)}
        />
        <span>{label(f.id, f.label)}</span>
      </label>
    {/each}
    {#if tags.length > 0}
      <label class="tag">
        <span>Tag</span>
        <Select
          size="sm"
          options={tagOptions}
          value={filters.tag ?? ""}
          onchange={(e) => set("tag", e.currentTarget.value || null)}
        />
      </label>
    {/if}
  </div>
</Popover>

<style>
  .toolbar {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2) 12px;
    padding: 6px 2px 8px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .tchip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 1.5rem;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-muted);
    padding: 2px 10px;
    border-radius: 999px;
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
  }
  .tchip:hover {
    color: var(--text);
  }
  .tchip[aria-pressed="true"] {
    background: color-mix(in srgb, var(--primary) 14%, transparent);
    border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
    color: var(--primary-text);
  }
  .filters-btn {
    display: inline-flex;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--sp-2) var(--sp-3);
    font-size: var(--fs-sm);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 1.75rem;
    cursor: pointer;
  }
  .opt input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--primary);
    cursor: inherit;
  }
  .opt.disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .tag {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  .count {
    font-size: var(--fs-xs);
    color: var(--text-muted);
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
    width: 28px;
    height: 24px;
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
