// `use:portal` moves an element to the end of `target` (default: document.body) so it escapes
// overflow/stacking contexts, and removes it again when the element is destroyed.
//   <div use:portal>...</div>
//   <div use:portal={someHost}>...</div>

/**
 * Svelte action: re-parent `node` under `target`.
 * @param node the element to move
 * @param target where to append it; defaults to document.body
 */
export function portal(node: HTMLElement, target?: HTMLElement | null) {
  function move(to: HTMLElement | null | undefined) {
    (to ?? document.body).appendChild(node);
  }
  move(target);
  return {
    update(next?: HTMLElement | null) {
      move(next);
    },
    destroy() {
      node.remove();
    },
  };
}
