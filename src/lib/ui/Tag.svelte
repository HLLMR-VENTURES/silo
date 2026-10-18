<script lang="ts">
  // A user's own tag on a mod: <Tag>harvest</Tag>. Neutral on purpose: tags are the user's
  // words, not Silo's verdicts (those are Badges).
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";

  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, "class"> {
    /** Show the tag glyph before the word. */
    showIcon?: boolean;
    class?: string;
    children?: Snippet;
  }

  let { showIcon = false, class: className = "", children, ...rest }: Props = $props();
</script>

<span {...rest} class="tag {className}">
  {#if showIcon}<Icon name="tag" size={12} />{/if}
  {@render children?.()}
</span>

<style>
  .tag {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    min-height: 1.25rem;
    padding: 0 var(--sp-2);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--muted);
    font-size: var(--fs-xs);
    line-height: var(--lh-xs);
    white-space: nowrap;
  }
</style>
