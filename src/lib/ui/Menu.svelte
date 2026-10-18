<script lang="ts">
  // Action menu on a Popover:
  //   <Menu bind:open anchor={btn} {items} onselect={(id) => run(id)} label="Mod actions" />
  //   <Menu bind:open point={{ x: e.clientX, y: e.clientY }} {items} onselect={...} />  (context menu)
  // Keyboard: focus starts on the first enabled item; ArrowUp/Down wrap; Home/End; type-ahead by
  // first letters (500 ms buffer); Enter/Space select; Esc closes and returns focus to the
  // trigger (anchor), or to what had focus before a point-opened menu; Tab closes.
  // For a button + menu pair use MenuButton.
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";
  import Popover from "./Popover.svelte";
  import type { MenuItem, Placement } from "./overlayTypes";

  interface Props {
    open?: boolean;
    anchor?: HTMLElement | null;
    point?: { x: number; y: number } | null;
    items: MenuItem[];
    onselect: (id: string) => void;
    placement?: Placement;
    /** Accessible name for the menu. */
    label?: string;
    id?: string;
    onclose?: () => void;
  }

  let {
    open = $bindable(false),
    anchor = null,
    point = null,
    items,
    onselect,
    placement = "bottom-start",
    label,
    id,
    onclose,
  }: Props = $props();

  let menuEl = $state<HTMLDivElement | null>(null);
  let opener: HTMLElement | null = null;
  let buffer = "";
  let bufferTimer: ReturnType<typeof setTimeout> | undefined;

  function itemEls(): HTMLElement[] {
    return menuEl ? Array.from(menuEl.querySelectorAll<HTMLElement>("[role='menuitem']")) : [];
  }
  function enabledEls(): HTMLElement[] {
    return itemEls().filter((el) => el.getAttribute("aria-disabled") !== "true");
  }

  $effect(() => {
    if (!open || !menuEl) return;
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    tick().then(() => enabledEls()[0]?.focus());
  });

  function choose(item: MenuItem) {
    if (item.disabled) return;
    open = false;
    // Focus goes back to the trigger first, so a dialog opened by the action remembers it.
    const back = anchor ?? opener;
    if (back?.isConnected) back.focus();
    onclose?.();
    onselect(item.id);
  }

  function move(delta: 1 | -1) {
    const list = enabledEls();
    if (!list.length) return;
    const i = list.indexOf(document.activeElement as HTMLElement);
    const next = i === -1 ? (delta === 1 ? 0 : list.length - 1) : (i + delta + list.length) % list.length;
    list[next].focus();
  }

  function typeahead(ch: string) {
    clearTimeout(bufferTimer);
    buffer += ch.toLowerCase();
    bufferTimer = setTimeout(() => (buffer = ""), 500);
    const list = enabledEls();
    if (!list.length) return;
    const cur = list.indexOf(document.activeElement as HTMLElement);
    // A repeated single letter cycles through matches; a longer buffer refines from the current item.
    const repeat = buffer.length > 1 && buffer.split("").every((c) => c === buffer[0]);
    const needle = repeat ? buffer[0] : buffer;
    const start = buffer.length === 1 || repeat ? cur + 1 : Math.max(cur, 0);
    for (let k = 0; k < list.length; k++) {
      const el = list[(start + k) % list.length];
      if ((el.textContent ?? "").trim().toLowerCase().startsWith(needle)) {
        el.focus();
        return;
      }
    }
  }

  function onkeydown(e: KeyboardEvent) {
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
        if (idx !== -1) choose(items[idx]);
        return;
      }
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && e.key !== " ") {
      e.preventDefault();
      typeahead(e.key);
    }
  }
</script>

<Popover
  bind:open
  bind:ref={menuEl}
  {anchor}
  {point}
  {placement}
  {id}
  role="menu"
  aria-label={label}
  aria-orientation="vertical"
  class="menu"
  {onkeydown}
  onclose={() => onclose?.()}
>
  {#each items as item (item.id)}
    {#if item.separatorBefore}<div class="sep" role="separator"></div>{/if}
    <button
      type="button"
      role="menuitem"
      tabindex="-1"
      class="item"
      class:danger={item.danger}
      aria-disabled={item.disabled ? "true" : undefined}
      aria-keyshortcuts={item.keyshortcuts}
      onclick={() => choose(item)}
    >
      <span class="icon">{#if item.icon}<Icon name={item.icon} size={16} />{/if}</span>
      <span class="label">{item.label}</span>
      {#if item.shortcut}<span class="shortcut" aria-hidden="true">{item.shortcut}</span>{/if}
    </button>
  {/each}
</Popover>

<style>
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
  .item.danger {
    color: var(--danger);
  }
  .item[aria-disabled="true"] {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .icon {
    display: inline-flex;
    width: 1rem;
    flex: none;
  }
  .label {
    flex: 1;
    white-space: nowrap;
  }
  .shortcut {
    margin-left: var(--sp-4);
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .sep {
    height: 1px;
    margin: var(--sp-1) 0;
    background: var(--border);
  }
</style>
