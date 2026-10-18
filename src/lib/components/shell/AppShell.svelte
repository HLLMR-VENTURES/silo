<script lang="ts">
  // The app frame: rail | header / notices / content, with an optional drawer docked right.
  //   <AppShell badges={{ updates: 4 }}>
  //     {#snippet header()}<AppHeader ... />{/snippet}
  //     {#snippet notices()}<NoticeArea />{/snippet}
  //     <LibraryView />
  //     {#snippet drawer()}<ModDetail ... />{/snippet}
  //   </AppShell>
  // Landmarks: <nav aria-label="Main"> (rail), <header>, <main id="main" tabindex="-1">, <aside>
  // (drawer). The first focusable element is a "Skip to content" link that focuses #main. Every
  // region carries data-region (rail, header, notices, main, drawer) for F6 cycling (./regions).
  // The rail collapses to icons below 1200px window width unless pinned (nav.rail); without a
  // `rail` snippet it renders NavRail bound to the nav state. Content is capped at --content-max
  // (1280px), or --content-max-wide (1920px) for the grid views that pass `wide`.
  import type { Snippet } from "svelte";
  import {
    go,
    nav,
    nextRailPin,
    railCollapsedFor,
    setRailPin,
    type RailPin,
    type ViewId,
  } from "../../state/nav.svelte";
  import NavRail from "./NavRail.svelte";

  interface Props {
    /** Replaces the default NavRail; receives whether the rail is collapsed. */
    rail?: Snippet<[{ collapsed: boolean }]>;
    header: Snippet;
    notices?: Snippet;
    /** The current view. */
    children?: Snippet;
    /** Docked right, as wide as its content; omit when no drawer is open. */
    drawer?: Snippet;
    /** Accessible name for the drawer's <aside>. */
    drawerLabel?: string;
    /** Badges for the default rail, e.g. { updates: 4 }. */
    badges?: Partial<Record<ViewId, number | string>>;
    /** Rail preference; defaults to the persisted nav.rail. */
    pin?: RailPin;
    /** The view manages its own scrolling (virtual lists): main doesn't scroll, content fills it. */
    fill?: boolean;
    /** Grid views (Library, Browse): content may grow to --content-max-wide instead of --content-max. */
    wide?: boolean;
  }

  let {
    rail,
    header,
    notices,
    children,
    drawer,
    drawerLabel = "Details",
    badges = {},
    pin,
    fill = false,
    wide = false,
  }: Props = $props();

  let width = $state(typeof window === "undefined" ? 1280 : window.innerWidth);
  const railPin = $derived(pin ?? nav.rail);
  const collapsed = $derived(railCollapsedFor(width, railPin));

  let mainEl = $state<HTMLElement | null>(null);

  function skipToContent(e: MouseEvent) {
    e.preventDefault();
    mainEl?.focus();
  }

  function togglePin() {
    setRailPin(nextRailPin(collapsed, width));
  }
</script>

<svelte:window bind:innerWidth={width} />

<div class="shell" class:collapsed class:has-drawer={!!drawer}>
  <a class="skip-link" href="#main" onclick={skipToContent}>Skip to content</a>

  <nav class="rail" aria-label="Main" data-region="rail">
    {#if rail}
      {@render rail({ collapsed })}
    {:else}
      <NavRail current={nav.view} {badges} {collapsed} onnavigate={(v) => go(v)} onTogglePin={togglePin} />
    {/if}
  </nav>

  <header class="header" data-region="header">
    {@render header()}
  </header>

  {#if notices}
    <div class="notices" data-region="notices">
      {@render notices()}
    </div>
  {/if}

  <main id="main" class="main" class:fill tabindex="-1" bind:this={mainEl} data-region="main">
    <div class="content" class:wide>
      {@render children?.()}
    </div>
  </main>

  {#if drawer}
    <aside class="drawer" aria-label={drawerLabel} data-region="drawer">
      {@render drawer()}
    </aside>
  {/if}
</div>

<style>
  .shell {
    --rail-current: var(--rail-w);
    display: grid;
    grid-template-columns: var(--rail-current) minmax(0, 1fr) auto;
    grid-template-rows: var(--header-h) auto minmax(0, 1fr);
    grid-template-areas:
      "rail header header"
      "rail notices notices"
      "rail main drawer";
    height: 100%;
    min-height: 0;
    background: var(--bg);
  }
  .shell.collapsed {
    --rail-current: var(--rail-w-collapsed);
  }

  .skip-link {
    position: absolute;
    top: var(--sp-2);
    left: var(--sp-2);
    z-index: var(--z-tooltip);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--text);
    font-weight: 600;
    text-decoration: none;
    box-shadow: var(--e2);
    transform: translateY(calc(-100% - var(--sp-4)));
  }
  .skip-link:focus {
    transform: none;
  }

  .rail {
    grid-area: rail;
    z-index: var(--z-rail);
    min-height: 0;
  }
  .header {
    grid-area: header;
    z-index: var(--z-header);
    min-width: 0;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .notices {
    grid-area: notices;
    min-width: 0;
  }
  .main {
    grid-area: main;
    min-width: 0;
    min-height: 0;
    overflow: auto;
  }
  .main:focus {
    outline: none;
  }
  .content {
    width: 100%;
    max-width: var(--content-max);
    min-height: 100%;
    margin: 0 auto;
    padding: var(--sp-5);
  }
  .content.wide {
    max-width: var(--content-max-wide);
  }
  .main.fill {
    overflow: hidden;
  }
  .main.fill .content {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  /* No width of its own: the auto column fits the drawer's child, which sizes itself (ModDetail
     via the resizable action). A fixed width here could disagree with the child's and spill. */
  .drawer {
    grid-area: drawer;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    z-index: var(--z-drawer);
    background: var(--surface);
    border-left: 1px solid var(--border);
  }
</style>
