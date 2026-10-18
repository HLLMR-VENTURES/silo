<script lang="ts" generics="T extends string">
  // Tabs with one panel: <Tabs {tabs} bind:selected ariaLabel="Mod details">{#snippet panel(id)}...{/snippet}</Tabs>.
  // Roving tabindex; Arrow Left/Right, Home, End move focus and select (automatic activation).
  import type { Snippet } from "svelte";
  import Badge from "./Badge.svelte";
  import Icon from "./Icon.svelte";
  import type { TabItem } from "./types";

  interface Props {
    tabs: TabItem<T>[];
    /** The selected tab id; defaults to the first tab. */
    selected?: T;
    ariaLabel?: string;
    panel: Snippet<[T]>;
    onchange?: (id: T) => void;
    class?: string;
  }

  let {
    tabs,
    selected = $bindable(),
    ariaLabel,
    panel,
    onchange,
    class: className = "",
  }: Props = $props();

  const uid = $props.id();
  const panelId = `${uid}-panel`;
  const tabId = (id: T) => `${uid}-tab-${id}`;

  let buttons: HTMLButtonElement[] = $state([]);

  const current = $derived(tabs.some((t) => t.id === selected) ? selected : tabs[0]?.id);
  const currentIndex = $derived(tabs.findIndex((t) => t.id === current));

  function select(i: number) {
    const t = tabs[i];
    if (!t || t.id === selected) return;
    selected = t.id;
    onchange?.(t.id);
  }

  function onkeydown(e: KeyboardEvent, i: number) {
    const n = tabs.length;
    let next: number;
    switch (e.key) {
      case "ArrowRight":
        next = (i + 1) % n;
        break;
      case "ArrowLeft":
        next = (i - 1 + n) % n;
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
    select(next);
    buttons[next]?.focus();
  }
</script>

<div class="tabs {className}">
  <div class="tablist" role="tablist" aria-label={ariaLabel}>
    {#each tabs as t, i (t.id)}
      <button
        bind:this={buttons[i]}
        type="button"
        role="tab"
        id={tabId(t.id)}
        class="tab"
        aria-selected={i === currentIndex}
        aria-controls={panelId}
        tabindex={i === currentIndex ? 0 : -1}
        onclick={() => select(i)}
        onkeydown={(e) => onkeydown(e, i)}
      >
        {#if t.icon}<Icon name={t.icon} size={16} />{/if}
        <span>{t.label}</span>
        {#if t.badge !== undefined}<Badge>{t.badge}</Badge>{/if}
      </button>
    {/each}
  </div>
  {#if current !== undefined}
    <div
      class="panel"
      role="tabpanel"
      id={panelId}
      aria-labelledby={tabId(current)}
      tabindex="0"
    >
      {@render panel(current)}
    </div>
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .tablist {
    display: flex;
    gap: var(--sp-1);
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
  }
  .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 2.25rem;
    padding: 0 var(--sp-3);
    border: 0;
    background: none;
    color: var(--muted);
    font: inherit;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab[aria-selected="true"] {
    color: var(--text);
  }
  .tab[aria-selected="true"]::after {
    content: "";
    position: absolute;
    left: var(--sp-2);
    right: var(--sp-2);
    bottom: -1px;
    height: 2px;
    border-radius: 2px 2px 0 0;
    background: var(--primary);
  }
  .panel {
    padding-top: var(--sp-4);
  }
  .panel:focus-visible {
    outline-offset: -2px;
  }
</style>
