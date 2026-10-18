// `use:tooltip={"Rescan library"}`: a short hint on hover (after 500 ms) or keyboard focus
// (at once), hidden again on leave, blur, click or Esc. The tip is a role="tooltip" element wired
// to the trigger's aria-describedby.
//
// A tooltip is a hint, never the only place essential information lives: it's invisible on touch,
// easy to miss, and gone the moment the pointer moves. Put anything the user needs in the UI itself.

const SHOW_DELAY_MS = 500;
const GAP = 6;
const MARGIN = 8;
let seq = 0;

const STYLE = [
  "position:fixed",
  "z-index:var(--z-tooltip)",
  "max-width:18rem",
  "padding:4px 8px",
  "border-radius:var(--r-sm)",
  "background:var(--text)",
  "color:var(--bg)",
  "font-family:var(--font-ui)",
  "font-size:var(--fs-xs)",
  "line-height:var(--lh-xs)",
  "pointer-events:none",
  "white-space:normal",
  "overflow-wrap:anywhere",
].join(";");

/**
 * Svelte action: attach a tooltip to `node`.
 * @param node the trigger element
 * @param text the tooltip text; empty / null disables it
 */
export function tooltip(node: HTMLElement, text: string | null | undefined) {
  const id = `silo-tooltip-${++seq}`;
  let current = text ?? "";
  let tip: HTMLDivElement | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function ensureTip(): HTMLDivElement {
    if (!tip) {
      tip = document.createElement("div");
      tip.id = id;
      tip.setAttribute("role", "tooltip");
      tip.hidden = true;
      tip.style.cssText = STYLE;
      document.body.appendChild(tip);
    }
    tip.textContent = current;
    return tip;
  }

  function describedBy(add: boolean) {
    const ids = (node.getAttribute("aria-describedby") ?? "").split(/\s+/).filter((t) => t && t !== id);
    if (add) ids.push(id);
    if (ids.length) node.setAttribute("aria-describedby", ids.join(" "));
    else node.removeAttribute("aria-describedby");
  }

  function position(el: HTMLDivElement) {
    const a = node.getBoundingClientRect();
    const t = el.getBoundingClientRect();
    let top = a.top - GAP - t.height;
    if (top < MARGIN) top = a.bottom + GAP;
    let left = a.left + a.width / 2 - t.width / 2;
    left = Math.max(MARGIN, Math.min(left, window.innerWidth - t.width - MARGIN));
    el.style.top = `${top}px`;
    el.style.left = `${left}px`;
  }

  function show() {
    clearTimeout(timer);
    if (!current) return;
    const el = ensureTip();
    el.hidden = false;
    position(el);
    document.addEventListener("keydown", onKey, true);
  }

  function hide() {
    clearTimeout(timer);
    if (tip) tip.hidden = true;
    document.removeEventListener("keydown", onKey, true);
  }

  function onKey(e: KeyboardEvent) {
    // Esc hides the tip without swallowing the key (a dialog underneath still gets it).
    if (e.key === "Escape") hide();
  }

  function onEnter() {
    clearTimeout(timer);
    timer = setTimeout(show, SHOW_DELAY_MS);
  }

  function attach() {
    if (!current) return;
    ensureTip();
    describedBy(true);
  }

  node.addEventListener("pointerenter", onEnter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("pointerdown", hide);
  node.addEventListener("focusin", show);
  node.addEventListener("focusout", hide);
  attach();

  return {
    update(next: string | null | undefined) {
      current = next ?? "";
      if (!current) {
        hide();
        describedBy(false);
        return;
      }
      attach();
      if (tip && !tip.hidden) position(tip);
    },
    destroy() {
      hide();
      node.removeEventListener("pointerenter", onEnter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("pointerdown", hide);
      node.removeEventListener("focusin", show);
      node.removeEventListener("focusout", hide);
      describedBy(false);
      tip?.remove();
      tip = null;
    },
  };
}
