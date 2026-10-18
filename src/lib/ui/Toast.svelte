<script lang="ts">
  // One toast inside ToastRegion. Owns its countdown: paused while hovered, while focus is inside,
  // and while the window is in the background (`windowActive` false).
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import type { IconName } from "./icons";
  import { dismissToast, type ToastItem } from "./toasts.svelte";

  let { item, windowActive = true }: { item: ToastItem; windowActive?: boolean } = $props();

  const ICON: Record<ToastItem["tone"], IconName> = {
    neutral: "info",
    success: "ok",
    warn: "problem",
    danger: "problem",
  };

  let hovered = $state(false);
  let focused = $state(false);
  const paused = $derived(hovered || focused || !windowActive);

  // svelte-ignore state_referenced_locally
  let remaining = item.duration;

  $effect(() => {
    if (paused) return;
    const started = Date.now();
    const timer = setTimeout(() => dismissToast(item.id), remaining);
    return () => {
      clearTimeout(timer);
      remaining = Math.max(0, remaining - (Date.now() - started));
    };
  });

  function runAction() {
    const action = item.action;
    dismissToast(item.id);
    if (!action) return;
    Promise.resolve()
      .then(() => action.run())
      .catch((e) => console.error("[Silo] toast action failed", e));
  }

  function onfocusout(e: FocusEvent) {
    const next = e.relatedTarget as Node | null;
    if (!next || !(e.currentTarget as HTMLElement).contains(next)) focused = false;
  }
</script>

<div
  class="toast tone-{item.tone}"
  role="group"
  aria-label="Notification"
  onpointerenter={() => (hovered = true)}
  onpointerleave={() => (hovered = false)}
  onfocusin={() => (focused = true)}
  {onfocusout}
>
  <span class="icon"><Icon name={ICON[item.tone]} size={16} /></span>
  <p class="message">{item.message}</p>
  {#if item.action}
    <Button size="sm" variant="ghost" onclick={runAction}>{item.action.label}</Button>
  {/if}
  <IconButton icon="close" label="Dismiss" size="sm" onclick={() => dismissToast(item.id)} />
</div>

<style>
  .toast {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-width: 16rem;
    max-width: 24rem;
    padding: var(--sp-2) var(--sp-2) var(--sp-2) var(--sp-3);
    border: 1px solid var(--border);
    border-left: 4px solid var(--border-strong);
    border-radius: var(--r-md);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--e2);
    pointer-events: auto;
    animation: rise var(--dur-2) var(--ease);
  }
  .tone-success {
    border-left-color: var(--success);
  }
  .tone-warn {
    border-left-color: var(--warn-text);
  }
  .tone-danger {
    border-left-color: var(--danger);
  }
  .icon {
    display: inline-flex;
    flex: none;
    color: var(--muted);
  }
  .tone-success .icon {
    color: var(--success);
  }
  .tone-warn .icon {
    color: var(--warn-text);
  }
  .tone-danger .icon {
    color: var(--danger);
  }
  .message {
    flex: 1;
    margin: 0;
    min-width: 0;
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
    overflow-wrap: anywhere;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .toast {
      animation-name: fade;
    }
  }
</style>
