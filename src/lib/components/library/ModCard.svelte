<script lang="ts" module>
  /** Card height, px (VirtualGrid's itemHeight; the grid adds the gap). */
  export const CARD_HEIGHT = 176;
</script>

<script lang="ts">
  // One library card, rendered inside a VirtualGrid item (which holds the focus and the keys).
  // Same rules as ModListRow: the Active switch (named "Active: <title>"), the ⋯ menu (also on
  // right-click) for everything else, at most three badges then "+n", a checkbox on hover/focus
  // (always once anything is selected), and parked cards desaturate the image, never the text.
  import Checkbox from "../../ui/Checkbox.svelte";
  import Icon from "../../ui/Icon.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import Switch from "../../ui/Switch.svelte";
  import { fmtSize } from "../../format";
  import type { ModEntry } from "../../types";
  import BadgeRow from "./BadgeRow.svelte";
  import { cachedModIcon, loadModIcon } from "./modIcon";
  import type { MenuTarget } from "./roving";
  import type { BadgeSpec } from "./rowBadges";

  interface Props {
    mod: ModEntry;
    active: boolean;
    badges: BadgeSpec[];
    selected?: boolean;
    selectionMode?: boolean;
    favorite?: boolean;
    flagged?: boolean;
    /** My star rating, 0–5. */
    rating?: number;
    menuOpen?: boolean;
    onToggle: () => void;
    onOpen: () => void;
    onMenu: (at: MenuTarget) => void;
    onSelect?: (id: string, intent: { shift: boolean; ctrl: boolean }) => void;
  }

  let {
    mod,
    active,
    badges,
    selected = false,
    selectionMode = false,
    favorite = false,
    flagged = false,
    rating = 0,
    menuOpen = false,
    onToggle,
    onOpen,
    onMenu,
    onSelect,
  }: Props = $props();

  const title = $derived(mod.title ?? mod.techName);
  const initial = $derived(title.trim().charAt(0).toUpperCase() || "?");
  const meta = $derived(
    [mod.version ? `v${mod.version.replace(/^v/i, "")}` : null, mod.category, fmtSize(mod.size)].filter(
      Boolean,
    ) as string[],
  );

  let iconSrc = $state<string | null>(null);
  $effect(() => {
    const m = mod;
    const hit = cachedModIcon(m);
    iconSrc = hit;
    if (hit || !m.iconFilename) return;
    let live = true;
    void loadModIcon(m).then((url) => {
      if (live && url) iconSrc = url;
    });
    return () => {
      live = false;
    };
  });

  function onclick(e: MouseEvent) {
    const t = e.target as HTMLElement | null;
    if (t?.closest("button, input, label, a")) return;
    onOpen();
  }

  function oncontextmenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    onMenu({ point: { x: e.clientX, y: e.clientY } });
  }

  function onCheck(e: MouseEvent) {
    e.preventDefault();
    onSelect?.(mod.path, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="card"
  class:active
  class:parked={!active}
  class:selected
  class:selecting={selectionMode || selected}
  class:flagged
  class:has-error={!!mod.error}
  {onclick}
  {oncontextmenu}
>
  <div class="head">
    <span class="check">
      <Checkbox checked={selected} aria-label="Select {title}" tabindex={-1} onclick={onCheck} />
    </span>
    <Switch checked={active} controlled ariaLabel="Active: {title}" tabindex={-1} onchange={onToggle} />
    <span class="word" class:on={active} aria-hidden="true">{active ? "Active" : "Parked"}</span>
    <span class="sp"></span>
    <IconButton
      icon="more"
      label="More actions for {title}"
      size="sm"
      tabindex={-1}
      aria-haspopup="menu"
      aria-expanded={menuOpen}
      onclick={(e) => onMenu({ anchor: e.currentTarget as HTMLElement })}
    />
  </div>

  <div class="body">
    {#if iconSrc}
      <img class="icon img" src={iconSrc} alt="" loading="lazy" />
    {:else}
      <span class="icon" class:map={mod.isMap} aria-hidden="true">{initial}</span>
    {/if}
    <div class="info">
      <div class="titleline">
        <span class="title">{title}</span>
        {#if favorite}<span class="fav" role="img" aria-label="Favorite"><Icon name="favorite" size={12} filled /></span>{/if}
      </div>
      <div class="author">{mod.author ?? "Unknown author"}</div>
      <div class="meta tnum">
        {#each meta as part, i (i)}{#if i > 0}<span class="dot" aria-hidden="true">·</span>{/if}<span>{part}</span>{/each}
      </div>
      {#if rating > 0}
        <span class="rating" role="img" aria-label="{rating} of 5 stars">
          {#each Array.from({ length: rating }) as _, i (i)}<Icon name="favorite" size={12} filled />{/each}
        </span>
      {/if}
    </div>
  </div>

  <div class="foot"><BadgeRow {badges} /></div>
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    height: 100%;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--surface);
    overflow: hidden;
    cursor: pointer;
    transition:
      border-color var(--dur-1) var(--ease),
      box-shadow var(--dur-1) var(--ease);
  }
  .card:hover {
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
    box-shadow: var(--e2);
  }
  .card.active {
    border-color: var(--primary);
  }
  .card.selected {
    background: color-mix(in srgb, var(--primary) 9%, var(--surface));
  }
  .card.has-error {
    background: var(--danger-soft);
  }
  .card.flagged {
    box-shadow: inset 3px 0 0 var(--danger);
  }

  .head {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-1) var(--sp-2) var(--sp-1) var(--sp-3);
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--sunken) 50%, transparent);
  }
  .check {
    display: inline-flex;
    opacity: 0;
    transition: opacity var(--dur-1) var(--ease);
  }
  .card:hover .check,
  .card.selecting .check,
  :global(.vg-item:focus-within) .check {
    opacity: 1;
  }
  .word {
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .word.on {
    color: var(--primary-text);
    font-weight: 600;
  }
  .sp {
    flex: 1 1 auto;
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    gap: var(--sp-3);
    padding: var(--sp-3);
  }
  .icon {
    flex: none;
    display: grid;
    place-items: center;
    width: 72px;
    height: 72px;
    border-radius: var(--r-sm);
    font-family: var(--font-display);
    font-size: var(--fs-2xl);
    font-weight: 600;
    color: var(--on-primary);
    background: linear-gradient(135deg, var(--primary), var(--primary-hover));
  }
  .icon.map {
    background: linear-gradient(135deg, var(--earth), var(--earth-strong));
    color: var(--on-earth);
  }
  .icon.img {
    object-fit: cover;
    background: var(--raised);
  }
  .parked .icon {
    filter: grayscale(1);
  }
  .info {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .titleline {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    min-width: 0;
  }
  .title {
    font-weight: 600;
    font-size: var(--fs-lg);
    line-height: var(--lh-lg);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fav,
  .rating {
    display: inline-flex;
    color: var(--accent-text);
  }
  .author,
  .meta {
    font-size: var(--fs-xs);
    line-height: var(--lh-xs);
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .dot {
    margin: 0 0.4em;
  }
  .foot {
    flex: none;
    display: flex;
    align-items: center;
    min-height: 2.25rem;
    padding: var(--sp-1) var(--sp-3);
    border-top: 1px solid var(--border);
  }

  @media (prefers-reduced-motion: reduce) {
    .card,
    .check {
      transition: none;
    }
  }
</style>
