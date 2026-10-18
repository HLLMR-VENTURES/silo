<script lang="ts">
  // Status word in a soft pill: <Badge tone="success" icon="verifiedBuild">Verified</Badge>.
  // Rows of badges: cap them with capBadges() from ./badges.
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, "class"> {
    tone?: "neutral" | "info" | "success" | "warn" | "danger" | "accent";
    icon?: IconName;
    class?: string;
    children?: Snippet;
  }

  let { tone = "neutral", icon, class: className = "", children, ...rest }: Props = $props();
</script>

<span {...rest} class="badge badge-{tone} {className}">
  {#if icon}<Icon name={icon} size={12} />{/if}
  {@render children?.()}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    min-height: 1.25rem;
    padding: 0 var(--sp-2);
    border-radius: var(--r-pill);
    font-size: var(--fs-xs);
    line-height: var(--lh-xs);
    font-weight: 500;
    white-space: nowrap;
  }
  .badge-neutral {
    background: var(--sunken);
    color: var(--muted);
  }
  .badge-info {
    background: var(--info-soft);
    color: var(--info);
  }
  .badge-success {
    background: var(--success-soft);
    color: var(--success);
  }
  .badge-warn {
    background: var(--warn-soft);
    color: var(--warn-text);
  }
  .badge-danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .badge-accent {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
</style>
