<script lang="ts">
  // Non-modal side panel (mod details, filters):
  //   <Drawer bind:open title={mod.title} bind:width onresize={saveWidth} returnFocus={() => rowEl(id)}>
  //     {#snippet header()}<IconButton icon="external" label="Open page" />{/snippet}
  //     ...content...
  //   </Drawer>
  // The page behind stays usable: no inert, no focus trap. Opening moves focus to the title;
  // Esc (while focus is inside) or the close button closes it and returns focus to
  // `returnFocus()` when given (so a caller can re-find a virtualized row), else to whatever
  // had focus when it opened. The inner-edge handle resizes (drag or Arrow keys); persisting the
  // width is the caller's job (`onresize`).
  import { tick, untrack, type Snippet } from "svelte";
  import IconButton from "./IconButton.svelte";

  const MIN_W = 360;
  const MAX_W = 720;
  const KEY_STEP = 16;

  interface Props {
    open?: boolean;
    title: string;
    side?: "right";
    /** Width in px, clamped to 360–720. */
    width?: number;
    /** Called when a resize gesture ends (or per Arrow key step) with the new width. */
    onresize?: (width: number) => void;
    onclose?: () => void;
    /** Where focus goes after Esc / close button. Defaults to the element focused before opening. */
    returnFocus?: () => HTMLElement | null;
    /** CSS length the drawer sits below. */
    top?: string;
    header?: Snippet;
    children?: Snippet;
    class?: string;
  }

  let {
    open = $bindable(false),
    title,
    side = "right",
    width = $bindable(440),
    onresize,
    onclose,
    returnFocus,
    top = "var(--header-h)",
    header,
    children,
    class: className = "",
  }: Props = $props();

  const uid = $props.id();
  const titleId = `${uid}-title`;

  let el = $state<HTMLElement | null>(null);
  let titleEl = $state<HTMLElement | null>(null);
  let opener: HTMLElement | null = null;

  const clamp = (w: number) => Math.round(Math.min(MAX_W, Math.max(MIN_W, w)));
  const shownWidth = $derived(clamp(width));

  $effect(() => {
    if (!open) return;
    untrack(() => {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    });
    tick().then(() => titleEl?.focus());
  });

  /** Close from inside (Esc / close button) and hand focus back. */
  function close() {
    if (!open) return;
    open = false;
    onclose?.();
    const target = returnFocus?.() ?? opener;
    opener = null;
    if (target?.isConnected) target.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      close();
    }
  }

  $effect(() => {
    if (!el) return;
    const node = el;
    node.addEventListener("keydown", onkeydown);
    return () => node.removeEventListener("keydown", onkeydown);
  });

  let drag: { x: number; w: number; id: number } | null = null;

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    e.preventDefault();
    drag = { x: e.clientX, w: shownWidth, id: e.pointerId };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }

  function onpointermove(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.id) return;
    // The handle is on the inner (left) edge of a right-side drawer: dragging left widens it.
    width = clamp(drag.w + (drag.x - e.clientX));
  }

  function onpointerup(e: PointerEvent) {
    if (!drag || e.pointerId !== drag.id) return;
    drag = null;
    onresize?.(shownWidth);
  }

  function onHandleKey(e: KeyboardEvent) {
    const delta = e.key === "ArrowLeft" ? KEY_STEP : e.key === "ArrowRight" ? -KEY_STEP : 0;
    if (!delta) return;
    e.preventDefault();
    width = clamp(shownWidth + delta);
    onresize?.(width);
  }
</script>

{#if open}
  <aside
    bind:this={el}
    class="drawer side-{side} {className}"
    aria-labelledby={titleId}
    style:width="{shownWidth}px"
    style:top
  >
    <!-- A focusable separator is the ARIA window-splitter pattern. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <div
      class="handle"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize panel"
      aria-valuenow={shownWidth}
      aria-valuemin={MIN_W}
      aria-valuemax={MAX_W}
      tabindex="0"
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
      onkeydown={onHandleKey}
    ></div>
    <header class="head">
      <h2 class="title" id={titleId} tabindex="-1" bind:this={titleEl}>{title}</h2>
      <div class="actions">
        {@render header?.()}
        <IconButton icon="close" label="Close panel" onclick={close} />
      </div>
    </header>
    <div class="body">
      {@render children?.()}
    </div>
  </aside>
{/if}

<style>
  .drawer {
    position: fixed;
    right: 0;
    bottom: 0;
    z-index: var(--z-drawer);
    display: flex;
    flex-direction: column;
    max-width: 100vw;
    border-left: 1px solid var(--border);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--e2);
    animation: slide-in var(--dur-3) var(--ease);
  }
  .handle {
    position: absolute;
    top: 0;
    bottom: 0;
    left: -4px;
    width: 8px;
    cursor: col-resize;
    touch-action: none;
  }
  .handle:hover,
  .handle:focus-visible {
    background: linear-gradient(to right, transparent 3px, var(--border-strong) 3px 5px, transparent 5px);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-3) var(--sp-3) var(--sp-3) var(--sp-5);
    border-bottom: 1px solid var(--border);
  }
  .title {
    flex: 1;
    min-width: 0;
    margin: 0;
    overflow: hidden;
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    line-height: var(--lh-xl);
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title:focus {
    outline: none;
  }
  .title:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
  }
  .body {
    flex: 1;
    min-height: 0;
    padding: var(--sp-4) var(--sp-5);
    overflow: auto;
  }

  @keyframes slide-in {
    from {
      opacity: 0;
      transform: translateX(24px);
    }
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .drawer {
      animation-name: fade;
    }
  }
</style>
