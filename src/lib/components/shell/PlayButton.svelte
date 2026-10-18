<script lang="ts" module>
  import type { MenuItem } from "../../ui/overlayTypes";

  /** One launch-menu entry, built by the caller. */
  export type PlayMenuItem = Pick<MenuItem, "id" | "label" | "icon" | "disabled" | "separatorBefore">;
</script>

<script lang="ts">
  // Header Play split button:
  //   <PlayButton count={142} onplay={launch} activeSetLine="Harvest 2026, unchanged"
  //     menuItems={[{ id: "save3", label: "Savegame 3 needs 2 parked mods: Turn them on" },
  //                 { id: "folder", label: "Open the game folder", icon: "showFile" }]}
  //     onmenuselect={run} disabled={!ready} disabledReason="Silo is moving mods." />
  // The main part launches. Disabled, it stays focusable (aria-disabled) so the reason reaches
  // everyone: a tooltip on hover/focus plus aria-describedby. The chevron opens the launch menu
  // (it stays usable while Play is blocked: it may hold the fix). `activeSetLine` heads the menu as
  // plain text, not an item. Menu keys: Arrow Up/Down wrap, Home/End, Enter/Space choose, Esc/Tab close.
  import { tick } from "svelte";
  import { formatCount } from "../../plural";
  import Icon from "../../ui/Icon.svelte";
  import Popover from "../../ui/Popover.svelte";
  import { tooltip } from "../../ui/tooltip";

  interface Props {
    count: number;
    disabled?: boolean;
    disabledReason?: string;
    loading?: boolean;
    onplay: () => void;
    menuItems?: PlayMenuItem[];
    onmenuselect?: (id: string) => void;
    /** "Harvest 2026, unchanged" / "Custom: 3 changes since Harvest 2026" / "No mod set". */
    activeSetLine?: string;
    open?: boolean;
  }

  let {
    count,
    disabled = false,
    disabledReason,
    loading = false,
    onplay,
    menuItems = [],
    onmenuselect,
    activeSetLine,
    open = $bindable(false),
  }: Props = $props();

  const uid = $props.id();
  const menuId = `${uid}-menu`;
  const reasonId = `${uid}-reason`;
  const headingId = `${uid}-heading`;

  let chevron = $state<HTMLButtonElement | null>(null);
  let menuEl = $state<HTMLDivElement | null>(null);

  const inert = $derived(disabled || loading);
  const showReason = $derived(disabled && !!disabledReason);

  function play() {
    if (inert) return;
    onplay();
  }

  function itemEls(): HTMLElement[] {
    return menuEl ? Array.from(menuEl.querySelectorAll<HTMLElement>("[role='menuitem']")) : [];
  }
  function enabledEls(): HTMLElement[] {
    return itemEls().filter((el) => el.getAttribute("aria-disabled") !== "true");
  }

  $effect(() => {
    if (!open || !menuEl) return;
    tick().then(() => enabledEls()[0]?.focus());
  });

  function choose(item: PlayMenuItem) {
    if (item.disabled) return;
    open = false;
    chevron?.focus();
    onmenuselect?.(item.id);
  }

  function move(delta: 1 | -1) {
    const list = enabledEls();
    if (!list.length) return;
    const i = list.indexOf(document.activeElement as HTMLElement);
    const next = i === -1 ? (delta === 1 ? 0 : list.length - 1) : (i + delta + list.length) % list.length;
    list[next].focus();
  }

  function menuKeydown(e: KeyboardEvent) {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        move(1);
        return;
      case "ArrowUp":
        e.preventDefault();
        move(-1);
        return;
      case "Home":
        e.preventDefault();
        enabledEls()[0]?.focus();
        return;
      case "End": {
        e.preventDefault();
        const list = enabledEls();
        list[list.length - 1]?.focus();
        return;
      }
      case "Enter":
      case " ": {
        e.preventDefault();
        const idx = itemEls().indexOf(document.activeElement as HTMLElement);
        if (idx !== -1) choose(menuItems[idx]);
        return;
      }
    }
  }

  function chevronKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      open = true;
    }
  }
</script>

<div class="play">
  <span class="main-wrap" use:tooltip={showReason ? disabledReason : null}>
    <button
      type="button"
      class="part main"
      aria-disabled={inert ? "true" : undefined}
      aria-busy={loading ? "true" : undefined}
      aria-describedby={showReason ? reasonId : undefined}
      onclick={play}
    >
      <span class="content" class:hidden={loading}>
        <Icon name="play" size={16} />
        <span>Play</span>
        <span aria-hidden="true">·</span>
        <span class="tnum">{formatCount(count)}</span>
        <span class="vh">mods</span>
      </span>
      {#if loading}<span class="spinner" aria-hidden="true"></span>{/if}
    </button>
  </span>
  <button
    bind:this={chevron}
    type="button"
    class="part chevron"
    aria-label="More launch options"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls={open ? menuId : undefined}
    onclick={() => (open = !open)}
    onkeydown={chevronKeydown}
  >
    <Icon name="chevronDown" size={16} />
  </button>
  {#if showReason}<span id={reasonId} class="vh">{disabledReason}</span>{/if}
</div>

<Popover
  bind:open
  bind:ref={menuEl}
  anchor={chevron}
  placement="bottom-end"
  id={menuId}
  role="menu"
  aria-label="Launch options"
  aria-describedby={activeSetLine ? headingId : undefined}
  aria-orientation="vertical"
  class="menu play-menu"
  onkeydown={menuKeydown}
>
  {#if activeSetLine}
    <div class="heading" id={headingId} role="none">
      <Icon name="modSets" size={16} />
      <span>{activeSetLine}</span>
    </div>
  {/if}
  {#each menuItems as item, i (item.id)}
    {#if item.separatorBefore || (i === 0 && activeSetLine)}<div class="sep" role="separator"></div>{/if}
    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      class="item"
      aria-disabled={item.disabled ? "true" : undefined}
      onclick={() => choose(item)}
    >
      <span class="item-icon">{#if item.icon}<Icon name={item.icon} size={16} />{/if}</span>
      <span class="label">{item.label}</span>
    </button>
  {/each}
</Popover>

<style>
  .play {
    display: inline-flex;
    align-items: stretch;
  }
  .main-wrap {
    display: inline-flex;
  }
  .part {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-height: 2.25rem;
    border: 0;
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--dur-1) var(--ease);
  }
  .part:hover:not([aria-disabled="true"]) {
    background: var(--primary-hover);
  }
  /* A --bg ring inside the global outline keeps the ring's contrast on the green fill. */
  .part:focus-visible {
    box-shadow: 0 0 0 2px var(--bg);
  }
  .main {
    gap: var(--sp-2);
    padding: 0 var(--sp-4);
    border-radius: var(--r-sm) 0 0 var(--r-sm);
  }
  .main[aria-disabled="true"] {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .main[aria-busy="true"] {
    opacity: 1;
    cursor: progress;
  }
  .chevron {
    padding: 0 var(--sp-2);
    border-left: 1px solid color-mix(in srgb, var(--on-primary) 35%, transparent);
    border-radius: 0 var(--r-sm) var(--r-sm) 0;
  }
  .content {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .content.hidden {
    visibility: hidden;
  }
  .spinner {
    position: absolute;
    inset: 0;
    margin: auto;
    width: 1em;
    height: 1em;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(1turn);
    }
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

  .heading {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3) var(--sp-2) var(--sp-2);
    color: var(--muted);
    font-size: var(--fs-sm);
    max-width: 22rem;
  }
  .item {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
    min-height: 2rem;
    padding: 0 var(--sp-3) 0 var(--sp-2);
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .item:hover:not([aria-disabled="true"]),
  .item:focus {
    background: var(--sunken);
  }
  .item:focus:not(:focus-visible) {
    outline: none;
  }
  .item[aria-disabled="true"] {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .item-icon {
    display: inline-flex;
    width: 1rem;
    flex: none;
  }
  .label {
    flex: 1;
    white-space: nowrap;
  }
  .sep {
    height: 1px;
    margin: var(--sp-1) 0;
    background: var(--border);
  }
  @media (prefers-reduced-motion: reduce) {
    .part {
      transition: none;
    }
    .spinner {
      animation-duration: 1.6s;
    }
  }
</style>
