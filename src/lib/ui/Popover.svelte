<script lang="ts">
  // Floating panel anchored to an element (or a point, for context menus):
  //   <Popover bind:open anchor={buttonEl} placement="bottom-end">...</Popover>
  // Portals out (into the top dialog when one is open), flips to stay in the viewport, follows
  // scroll/resize. Closes on outside pointerdown, on Esc (focus returns to the anchor, or to
  // whatever had focus when it opened), and when focus Tabs out of it.
  // Extra attributes (role, aria-label, onkeydown) land on the floating element itself.
  import { tick, untrack, type Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import { portal } from "./portal";
  import { portalTarget, pushLayer } from "./overlayStack";
  import type { Placement, PopoverCloseReason } from "./overlayTypes";

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, "class" | "children" | "onclose"> {
    open?: boolean;
    anchor?: HTMLElement | null;
    /** Viewport coordinates to open at instead of an anchor (context menus). */
    point?: { x: number; y: number } | null;
    placement?: Placement;
    /** Gap between anchor and panel, px. */
    offset?: number;
    onclose?: (reason: PopoverCloseReason) => void;
    /** The floating element. */
    ref?: HTMLDivElement | null;
    class?: string;
    children?: Snippet;
  }

  let {
    open = $bindable(false),
    anchor = null,
    point = null,
    placement = "bottom-start",
    offset = 4,
    onclose,
    ref = $bindable(null),
    class: className = "",
    children,
    ...rest
  }: Props = $props();

  const MARGIN = 8;
  let pos = $state({ x: -9999, y: -9999 });
  let target = $state<HTMLElement | null>(null);
  let opener: HTMLElement | null = null;

  function anchorRect(): { left: number; right: number; top: number; bottom: number } | null {
    if (point) return { left: point.x, right: point.x, top: point.y, bottom: point.y };
    return anchor?.getBoundingClientRect() ?? null;
  }

  function place() {
    if (!ref) return;
    const a = anchorRect();
    if (!a) return;
    const gap = point ? 0 : offset;
    const { width: w, height: h } = ref.getBoundingClientRect();
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const [vert, horiz] = placement.split("-") as ["top" | "bottom", "start" | "end"];

    const below = a.bottom + gap;
    const above = a.top - gap - h;
    let y = vert === "bottom" ? below : above;
    if (vert === "bottom" && below + h > vh - MARGIN && above >= MARGIN) y = above;
    if (vert === "top" && above < MARGIN && below + h <= vh - MARGIN) y = below;

    const startX = a.left;
    const endX = a.right - w;
    let x = horiz === "start" ? startX : endX;
    if (horiz === "start" && startX + w > vw - MARGIN && endX >= MARGIN) x = endX;
    if (horiz === "end" && endX < MARGIN && startX + w <= vw - MARGIN) x = startX;

    pos = {
      x: Math.max(MARGIN, Math.min(x, vw - w - MARGIN)),
      y: Math.max(MARGIN, Math.min(y, vh - h - MARGIN)),
    };
  }

  function close(reason: PopoverCloseReason) {
    if (!open) return;
    open = false;
    onclose?.(reason);
    if (reason === "escape") {
      const back = anchor ?? opener;
      if (back?.isConnected) back.focus();
    }
  }

  $effect(() => {
    if (!open) return;
    untrack(() => {
      opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      target = portalTarget();
    });
  });

  $effect(() => {
    if (!open || !ref) return;
    const node = ref;
    const layer = pushLayer({ host: node, modal: false, onEscape: () => close("escape") });

    const onPointerDown = (e: PointerEvent) => {
      const t = e.target as Node | null;
      if (!t || node.contains(t) || untrack(() => anchor)?.contains(t)) return;
      close("outside");
    };
    const onFocusOut = (e: FocusEvent) => {
      const next = e.relatedTarget as Node | null;
      if (next && node.contains(next)) return;
      // Focus moving to nowhere (e.g. a click on page background) is handled by pointerdown.
      if (next) close("blur");
    };
    const reflow = () => place();

    document.addEventListener("pointerdown", onPointerDown, true);
    node.addEventListener("focusout", onFocusOut);
    window.addEventListener("scroll", reflow, true);
    window.addEventListener("resize", reflow);
    untrack(place);
    tick().then(() => untrack(place));

    return () => {
      layer.pop();
      document.removeEventListener("pointerdown", onPointerDown, true);
      node.removeEventListener("focusout", onFocusOut);
      window.removeEventListener("scroll", reflow, true);
      window.removeEventListener("resize", reflow);
    };
  });

  // Re-place when the anchor/point/placement changes while open.
  $effect(() => {
    void anchor;
    void point;
    void placement;
    if (open && ref) untrack(place);
  });
</script>

{#if open && target}
  <div
    tabindex="-1"
    {...rest}
    bind:this={ref}
    use:portal={target}
    class="popover {className}"
    style:left="{pos.x}px"
    style:top="{pos.y}px"
  >
    {@render children?.()}
  </div>
{/if}

<style>
  .popover {
    position: fixed;
    z-index: var(--z-popover);
    min-width: 10rem;
    max-width: min(24rem, calc(100vw - 16px));
    max-height: calc(100vh - 16px);
    overflow: auto;
    padding: var(--sp-1);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--e2);
    animation: pop var(--dur-1) var(--ease);
  }
  .popover:focus {
    outline: none;
  }
  @keyframes pop {
    from {
      opacity: 0;
    }
  }
</style>
