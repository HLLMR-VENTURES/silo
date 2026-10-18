<script lang="ts">
  // The left rail's contents (AppShell wraps it in <nav aria-label="Main">):
  //   <NavRail current={nav.view} badges={{ updates: 4 }} {collapsed} onnavigate={go} onTogglePin={flip} />
  // Arrow Up/Down (Home/End) move focus between destinations (one tab stop, roving tabindex);
  // Enter/Space activate. Collapsed, items show icons only: the label lives in aria-label and a
  // tooltip. The current item has aria-current="page" and a gold fill bar (fill, never text).
  import Badge from "../../ui/Badge.svelte";
  import Icon from "../../ui/Icon.svelte";
  import SiloMark from "../../ui/SiloMark.svelte";
  import { tooltip } from "../../ui/tooltip";
  import type { ViewId } from "../../state/nav.svelte";
  import { hasBadge, NAV_ITEMS, navItemName, type NavItem } from "./navItems";

  interface Props {
    items?: readonly NavItem[];
    current: ViewId;
    badges?: Partial<Record<ViewId, number | string>>;
    collapsed?: boolean;
    onnavigate: (view: ViewId) => void;
    onTogglePin?: () => void;
  }

  let {
    items = NAV_ITEMS,
    current,
    badges = {},
    collapsed = false,
    onnavigate,
    onTogglePin,
  }: Props = $props();

  const main = $derived(items.filter((i) => i.group === "main"));
  const bottom = $derived(items.filter((i) => i.group === "bottom"));
  /** Flat order for arrow keys: main group, then bottom group. */
  const ordered = $derived([...main, ...bottom]);

  let buttons: HTMLButtonElement[] = $state([]);
  /** The item that holds the tab stop; follows focus, falls back to the current view. */
  let focusId = $state<ViewId | null>(null);
  const tabStop = $derived.by(() => {
    if (focusId && ordered.some((i) => i.id === focusId)) return focusId;
    return ordered.some((i) => i.id === current) ? current : ordered[0]?.id;
  });

  function onkeydown(e: KeyboardEvent, index: number) {
    const n = ordered.length;
    let next: number;
    switch (e.key) {
      case "ArrowDown":
        next = (index + 1) % n;
        break;
      case "ArrowUp":
        next = (index - 1 + n) % n;
        break;
      case "Home":
        next = 0;
        break;
      case "End":
        next = n - 1;
        break;
      default:
        return;
    }
    e.preventDefault();
    focusId = ordered[next].id;
    buttons[next]?.focus();
  }

  const pinLabel = $derived(collapsed ? "Expand sidebar" : "Collapse sidebar");
</script>

{#snippet item(entry: NavItem, index: number)}
  {@const badge = badges[entry.id]}
  {@const name = navItemName(entry, badge)}
  <li>
    <button
      bind:this={buttons[index]}
      type="button"
      class="item"
      aria-current={entry.id === current ? "page" : undefined}
      aria-label={name}
      tabindex={entry.id === tabStop ? 0 : -1}
      use:tooltip={collapsed ? entry.label : null}
      onfocus={() => (focusId = entry.id)}
      onclick={() => onnavigate(entry.id)}
      onkeydown={(e) => onkeydown(e, index)}
    >
      <span class="icon"><Icon name={entry.icon} size={20} /></span>
      {#if !collapsed}<span class="label" aria-hidden="true">{entry.label}</span>{/if}
      {#if hasBadge(badge)}
        {#if collapsed}
          <span class="dot" aria-hidden="true"></span>
        {:else}
          <Badge tone="accent" class="tnum count" aria-hidden="true">{badge}</Badge>
        {/if}
      {/if}
    </button>
  </li>
{/snippet}

<div class="rail" class:collapsed>
  <div class="brand">
    {#if collapsed}
      <SiloMark variant="tile" size={32} label={false} />
    {:else}
      <SiloMark variant="wordmark" size={28} label={false} />
    {/if}
  </div>

  <ul class="group main-group">
    {#each main as entry, i (entry.id)}
      {@render item(entry, i)}
    {/each}
  </ul>

  <ul class="group bottom-group">
    {#each bottom as entry, i (entry.id)}
      {@render item(entry, main.length + i)}
    {/each}
  </ul>

  {#if onTogglePin}
    <div class="pin">
      <button
        type="button"
        class="item pin-btn"
        aria-label={pinLabel}
        use:tooltip={collapsed ? pinLabel : null}
        onclick={() => onTogglePin?.()}
      >
        <span class="icon"><Icon name="menu" size={20} /></span>
        {#if !collapsed}<span class="label" aria-hidden="true">{pinLabel}</span>{/if}
      </button>
    </div>
  {/if}
</div>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: var(--sp-3) var(--sp-2);
    gap: var(--sp-1);
    background: var(--surface);
    border-right: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
  }
  .brand {
    display: flex;
    align-items: center;
    min-height: 2.5rem;
    padding: 0 var(--sp-2);
    margin-bottom: var(--sp-3);
  }
  .collapsed .brand {
    justify-content: center;
    padding: 0;
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .bottom-group {
    margin-top: auto;
    padding-top: var(--sp-3);
  }
  .item {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    width: 100%;
    min-height: 2.5rem;
    padding: 0 var(--sp-3);
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    transition:
      background-color var(--dur-1) var(--ease),
      color var(--dur-1) var(--ease);
  }
  .collapsed .item {
    justify-content: center;
    padding: 0;
  }
  .item:hover {
    background: var(--sunken);
    color: var(--text);
  }
  .item[aria-current="page"] {
    background: var(--sunken);
    color: var(--text);
    font-weight: 600;
  }
  /* The selected marker: a gold fill bar on the leading edge. */
  .item[aria-current="page"]::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0.5rem;
    bottom: 0.5rem;
    width: 3px;
    border-radius: 0 2px 2px 0;
    background: var(--accent);
  }
  .icon {
    display: inline-flex;
    flex: none;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item :global(.count) {
    margin-left: auto;
  }
  .dot {
    position: absolute;
    top: 0.45rem;
    right: 0.7rem;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: var(--r-pill);
    background: var(--accent);
    box-shadow: 0 0 0 2px var(--surface);
  }
  .pin {
    padding-top: var(--sp-2);
    margin-top: var(--sp-2);
    border-top: 1px solid var(--border);
  }
  .pin-btn {
    font-weight: 400;
    font-size: var(--fs-sm);
  }
  @media (prefers-reduced-motion: reduce) {
    .item {
      transition: none;
    }
  }
</style>
