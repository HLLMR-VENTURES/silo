// Svelte action: make a right-anchored drawer panel horizontally resizable by dragging
// its left edge. Width persists in localStorage under a shared key so the Library and
// Browse drawers stay the same width.
//
// The grab handle is a fixed-position element on <body>, kept aligned to the drawer's
// left edge — not a child of the drawer. That way it never scrolls away inside a
// scrollable drawer and is never clipped by the drawer's overflow. Its base styling
// lives in global.css as `.drawer-resize-handle` (top/left/height are set here). It follows the
// drawer through a ResizeObserver (and window resizes), so it stays on the edge when a notice or
// the rail changes the layout around the drawer.

interface Opts {
  min?: number;
  max?: number;
  storageKey?: string;
}

export function resizable(node: HTMLElement, opts: Opts = {}) {
  const min = opts.min ?? 340;
  const key = opts.storageKey ?? "silo.drawerWidth";
  const maxOf = () => opts.max ?? Math.round(window.innerWidth * 0.7);

  const handle = document.createElement("div");
  handle.className = "drawer-resize-handle";
  handle.setAttribute("aria-hidden", "true");
  document.body.appendChild(handle);

  const sync = () => {
    const r = node.getBoundingClientRect();
    handle.style.top = `${r.top}px`;
    handle.style.height = `${r.height}px`;
    handle.style.left = `${r.left - 3}px`;
    // Publish the live drawer width so the view can reserve space beside it (the top
    // chrome + list tuck to the drawer's left instead of hiding under it).
    document.documentElement.style.setProperty("--drawer-w", `${Math.round(r.width)}px`);
  };

  const apply = (w: number) => {
    node.style.width = `${Math.max(min, Math.min(maxOf(), w))}px`;
    sync();
  };

  const stored = Number(localStorage.getItem(key));
  if (stored) apply(stored);
  else sync();

  let dragging = false;
  let startX = 0;
  let startW = 0;

  const onDown = (e: PointerEvent) => {
    dragging = true;
    startX = e.clientX;
    startW = node.offsetWidth;
    handle.setPointerCapture(e.pointerId);
    document.body.style.userSelect = "none";
    document.body.style.cursor = "ew-resize";
    e.preventDefault();
  };
  const onMove = (e: PointerEvent) => {
    if (!dragging) return;
    // Anchored to the right edge, so dragging left (smaller clientX) widens it.
    apply(startW + (startX - e.clientX));
  };
  const onUp = () => {
    if (!dragging) return;
    dragging = false;
    document.body.style.userSelect = "";
    document.body.style.cursor = "";
    localStorage.setItem(key, String(node.offsetWidth));
  };

  handle.addEventListener("pointerdown", onDown);
  window.addEventListener("pointermove", onMove);
  window.addEventListener("pointerup", onUp);
  window.addEventListener("resize", sync);
  // The drawer moves or resizes without a window resize when the notices above it or the rail
  // beside the list change: watch the drawer itself (and its container, whose height it fills).
  const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => sync());
  observer?.observe(node);
  if (node.parentElement) observer?.observe(node.parentElement);
  // Re-align once layout has settled (fonts, wrap, transitions).
  requestAnimationFrame(sync);

  return {
    destroy() {
      handle.removeEventListener("pointerdown", onDown);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("resize", sync);
      observer?.disconnect();
      handle.remove();
      // Drawer closed — release the reserved space.
      document.documentElement.style.setProperty("--drawer-w", "0px");
    },
  };
}
