// Module-level stack of open overlays (dialogs, popovers, menus). It owns three shared behaviors
// so nested overlays stay consistent:
//   - Escape: one window listener (capture phase) hands Esc to the TOP layer only.
//   - inert: while any modal layer is open, every <body> child except the top modal's host is
//     made `inert` (so #app, lower dialogs and stray portals can't be clicked or tabbed into).
//     Elements marked `data-overlay-exempt` (the toast live region) are left alone.
//   - focus containment: focus that lands outside the top modal's host is pulled back in.

export interface OverlayLayer {
  /** The layer's root element (for a modal: the portaled host holding scrim + panel). */
  host: HTMLElement;
  /** Modal layers make the rest of the page inert and contain focus. */
  modal: boolean;
  /** Called when Esc is pressed while this is the top layer. Omit to swallow Esc (modal) or let it pass (non-modal). */
  onEscape?: () => void;
}

export interface OverlayHandle {
  /** Remove this layer (idempotent). */
  pop(): void;
}

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
  "[contenteditable='true']",
].join(",");

const stack: OverlayLayer[] = [];
/** Elements this module set `inert` on, so we never strip an `inert` someone else set. */
const madeInert = new Set<Element>();
let listening = false;

/**
 * The tabbable elements inside `root`, in DOM order (skips anything under an `inert` or `hidden` ancestor).
 * @param root container to search
 * @returns the focusable elements
 */
export function focusableIn(root: ParentNode): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (el) => !el.closest("[inert], [hidden]") && el.getAttribute("aria-hidden") !== "true",
  );
}

function topModal(): OverlayLayer | undefined {
  for (let i = stack.length - 1; i >= 0; i--) if (stack[i].modal) return stack[i];
  return undefined;
}

/**
 * Where a new popover/tooltip should portal to: inside the top modal's host when a modal is open
 * (so it stays interactive and inside the focus containment), else document.body.
 */
export function portalTarget(): HTMLElement {
  return topModal()?.host ?? document.body;
}

/** True when `layer` is the topmost open overlay. */
export function isTopLayer(layer: OverlayLayer): boolean {
  return stack[stack.length - 1] === layer;
}

function applyInert() {
  for (const el of madeInert) el.removeAttribute("inert");
  madeInert.clear();
  const top = topModal();
  if (!top) return;
  for (const child of Array.from(document.body.children)) {
    if (child === top.host || child.contains(top.host)) continue;
    if (child.hasAttribute("data-overlay-exempt") || child.hasAttribute("inert")) continue;
    if (child instanceof HTMLScriptElement || child instanceof HTMLStyleElement) continue;
    child.setAttribute("inert", "");
    madeInert.add(child);
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Escape" || e.defaultPrevented) return;
  const top = stack[stack.length - 1];
  if (!top) return;
  if (top.onEscape) {
    e.preventDefault();
    e.stopPropagation();
    top.onEscape();
  } else if (top.modal) {
    // A non-dismissible modal still owns Esc: nothing underneath should react.
    e.stopPropagation();
  }
}

function onFocusIn(e: FocusEvent) {
  const top = topModal();
  const target = e.target as Node | null;
  if (!top || !target || top.host.contains(target)) return;
  // Anything above the top modal (later in the stack) is allowed to hold focus.
  const idx = stack.indexOf(top);
  if (stack.slice(idx + 1).some((l) => l.host.contains(target))) return;
  const panel = top.host.querySelector<HTMLElement>("[role='dialog'], [role='alertdialog']");
  (focusableIn(top.host)[0] ?? panel ?? top.host).focus();
}

function sync() {
  const want = stack.length > 0;
  if (want && !listening) {
    window.addEventListener("keydown", onKeydown, true);
    document.addEventListener("focusin", onFocusIn, true);
    listening = true;
  } else if (!want && listening) {
    window.removeEventListener("keydown", onKeydown, true);
    document.removeEventListener("focusin", onFocusIn, true);
    listening = false;
  }
  applyInert();
}

/**
 * Push an overlay onto the stack.
 * @param layer the overlay's host element, modality and Esc handler
 * @returns a handle whose `pop()` removes it again
 */
export function pushLayer(layer: OverlayLayer): OverlayHandle {
  stack.push(layer);
  sync();
  return {
    pop() {
      const i = stack.indexOf(layer);
      if (i === -1) return;
      stack.splice(i, 1);
      sync();
    },
  };
}

/** Number of open overlays (tests and debugging). */
export function layerCount(): number {
  return stack.length;
}
