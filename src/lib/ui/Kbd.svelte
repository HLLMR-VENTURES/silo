<script lang="ts">
  // Keyboard key: <Kbd>Ctrl</Kbd>+<Kbd>K</Kbd>.
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";

  interface Props extends Omit<HTMLAttributes<HTMLElement>, "class"> {
    class?: string;
    children?: Snippet;
  }

  let { class: className = "", children, ...rest }: Props = $props();
</script>

<kbd {...rest} class="kbd {className}">{@render children?.()}</kbd>

<style>
  .kbd {
    display: inline-block;
    min-width: 1.5em;
    padding: 0 var(--sp-1);
    border: 1px solid var(--border);
    border-bottom: 2px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--sunken);
    color: var(--text);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: 1.6;
    text-align: center;
    white-space: nowrap;
  }
</style>
