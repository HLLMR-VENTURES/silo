<script lang="ts" module>
  export type Density = "comfortable" | "compact";
  /** Row heights per density, px (VirtualList's rowHeight). */
  export const ROW_HEIGHTS: Record<Density, number> = { comfortable: 56, compact: 44 };
</script>

<script lang="ts">
  // One library row, rendered inside a VirtualList item (which holds the focus and the keys):
  //   [☐] [icon] Courseplay                     [Verified build] [Update]  (●━) Active  ⋯
  //              Courseplay Dev Team · 8.0.0.4 · Scripts & Tools · 6.2 MB
  // Inline: the selection checkbox (on hover/focus, always once anything is selected), the Active
  // switch and the ⋯ menu. Everything else (favorite, hide, flag, category, settings, Find in
  // Browse) lives in the ⋯ / right-click menu. Parked rows desaturate the icon and say "Parked";
  // their text stays full strength. Inner controls are tabindex -1: the row is the tab stop.
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
    /** From rowBadges(); capped at three here. */
    badges: BadgeSpec[];
    selected?: boolean;
    /** Something is selected: keep every checkbox visible. */
    selectionMode?: boolean;
    density?: Density;
    favorite?: boolean;
    flagged?: boolean;
    /** This row's menu is open (aria-expanded on ⋯). */
    menuOpen?: boolean;
    /** Flip Active (App's toggleActive). */
    onToggle: () => void;
    /** Open the details drawer. */
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
    density = "comfortable",
    favorite = false,
    flagged = false,
    menuOpen = false,
    onToggle,
    onOpen,
    onMenu,
    onSelect,
  }: Props = $props();

  const title = $derived(mod.title ?? mod.techName);
  const initial = $derived(title.trim().charAt(0).toUpperCase() || "?");
  const meta = $derived(
    [
      mod.author ?? "Unknown author",
      mod.version ? mod.version.replace(/^v/i, "") : null,
      mod.category,
      fmtSize(mod.size),
    ].filter(Boolean) as string[],
  );

  // Lazy icon: rows only mount on screen, so only on-screen icons load. A response that lands
  // after the row moved on to another mod is dropped.
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

  // A click on the row body opens the drawer; clicks on its controls are theirs.
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
    // The parent owns `selected`: the click only asks.
    e.preventDefault();
    onSelect?.(mod.path, { shift: e.shiftKey, ctrl: e.ctrlKey || e.metaKey });
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div
  class="row {density}"
  class:parked={!active}
  class:selected
  class:selecting={selectionMode || selected}
  class:flagged
  class:has-error={!!mod.error}
  {onclick}
  {oncontextmenu}
>
  <span class="check">
    <Checkbox checked={selected} aria-label="Select {title}" tabindex={-1} onclick={onCheck} />
  </span>

  {#if iconSrc}
    <img class="icon img" src={iconSrc} alt="" loading="lazy" />
  {:else}
    <span class="icon" class:map={mod.isMap} aria-hidden="true">{initial}</span>
  {/if}

  <span class="main">
    <span class="titleline">
      <span class="title">{title}</span>
      {#if favorite}<span class="fav" role="img" aria-label="Favorite"><Icon name="favorite" size={12} filled /></span>{/if}
    </span>
    <span class="meta tnum">
      {#each meta as part, i (i)}{#if i > 0}<span class="dot" aria-hidden="true">·</span>{/if}<span>{part}</span>{/each}
    </span>
  </span>

  <span class="badges"><BadgeRow {badges} /></span>

  <span class="state">
    <Switch checked={active} controlled ariaLabel="Active: {title}" tabindex={-1} onchange={onToggle} />
    <span class="word" class:on={active} aria-hidden="true">{active ? "Active" : "Parked"}</span>
  </span>

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

<style>
  .row {
    /* A container, so a narrow list drops the badges before squeezing the title away. */
    container-type: inline-size;
    display: grid;
    grid-template-columns: 1.5rem auto minmax(0, 1fr) auto auto auto;
    align-items: center;
    column-gap: var(--sp-3);
    height: 100%;
    padding: 0 var(--sp-2) 0 var(--sp-3);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
  }
  .row:hover {
    background: color-mix(in srgb, var(--primary) 5%, transparent);
  }
  .row.selected {
    background: color-mix(in srgb, var(--primary) 11%, transparent);
  }
  .row.has-error {
    background: var(--danger-soft);
  }
  .row.flagged {
    box-shadow: inset 3px 0 0 var(--danger);
  }

  /* The checkbox shows on hover/focus, and everywhere once anything is selected. Only the control
     fades; the row's text never does. */
  .check {
    display: inline-flex;
    opacity: 0;
    transition: opacity var(--dur-1) var(--ease);
  }
  .row:hover .check,
  .row.selecting .check,
  :global(.vg-item:focus-within) .check {
    opacity: 1;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border-radius: var(--r-sm);
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--on-primary);
    background: linear-gradient(135deg, var(--primary), var(--primary-hover));
  }
  .compact .icon {
    width: 28px;
    height: 28px;
    font-size: var(--fs-md);
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

  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .titleline {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    min-width: 0;
  }
  .title {
    font-weight: 600;
    font-size: var(--fs-md);
    line-height: var(--lh-md);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fav {
    display: inline-flex;
    color: var(--accent-text);
  }
  .meta {
    color: var(--muted);
    font-size: var(--fs-xs);
    line-height: var(--lh-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dot {
    margin: 0 0.4em;
  }
  .compact .title {
    line-height: 1.25;
  }
  .compact .meta {
    line-height: 1.2;
  }

  .badges {
    display: flex;
    justify-content: flex-end;
    min-width: 0;
  }
  @container (max-width: 640px) {
    .badges {
      display: none;
    }
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .word {
    min-width: 3.25rem;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .word.on {
    color: var(--primary-text);
    font-weight: 600;
  }

  @media (prefers-reduced-motion: reduce) {
    .check {
      transition: none;
    }
  }
</style>
