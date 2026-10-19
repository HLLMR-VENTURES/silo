<script lang="ts">
  // One toast inside ToastRegion. Owns its countdown: paused while hovered, while focus is inside,
  // and while the window is in the background (`windowActive` false). While a dialog is open its
  // buttons are inert (the region itself stays exempt so its text is still announced).
  // Sticky toasts (errors) have no countdown. `detail` adds "Copy details"; a count above 1
  // shows as "×3".
  import { copyText, saveTextFile } from "../api";
  import { copyOrSave } from "../clipboard";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import IconButton from "./IconButton.svelte";
  import type { IconName } from "./icons";
  import { modalOpen } from "./overlayStack";
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
  /** The action's accessible name says what it acts on: "Undo: Switched to Harvest." */
  const actionName = $derived(
    item.action ? (item.action.ariaLabel ?? `${item.action.label}: ${item.message}`) : undefined,
  );

  // svelte-ignore state_referenced_locally
  let remaining = item.duration;

  $effect(() => {
    if (paused || item.sticky) return;
    const started = Date.now();
    const timer = setTimeout(() => dismissToast(item.id), remaining);
    return () => {
      clearTimeout(timer);
      remaining = Math.max(0, remaining - (Date.now() - started));
    };
  });

  /** What Copy details did: copied to the clipboard, or saved to a file when it couldn't. */
  let copied = $state<false | "copied" | "saved">(false);
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => () => clearTimeout(copiedTimer));

  async function copyDetails() {
    if ($modalOpen || !item.detail) return;
    try {
      const out = await copyOrSave(item.detail, "silo-error-details.txt", copyText, saveTextFile);
      if (out === "cancelled") return;
      copied = out;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } catch {
      // Saving failed too; nothing more to offer here.
    }
  }

  function runAction() {
    if ($modalOpen) return; // inert already blocks it; this covers webviews without inert
    const action = item.action;
    dismissToast(item.id);
    if (!action) return;
    Promise.resolve()
      .then(() => action.run())
      .catch((e) => console.error("[Silo] toast action failed", e));
  }

  /** Sets / removes the `inert` attribute itself (engines without the `inert` property still see it). */
  function inertWhen(node: HTMLElement, on: boolean) {
    const apply = (v: boolean) => (v ? node.setAttribute("inert", "") : node.removeAttribute("inert"));
    apply(on);
    return { update: apply };
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
  <div class="text">
    <p class="message">
      {item.message}
      {#if item.count > 1}<span class="count" title="Happened {item.count} times">×{item.count}</span>{/if}
    </p>
    {#if item.body}<p class="body">{item.body}</p>{/if}
  </div>
  <span class="controls" use:inertWhen={$modalOpen}>
    {#if item.detail}
      <Button size="sm" variant="ghost" onclick={copyDetails}>{copied === "saved" ? "Saved to a file" : copied ? "Copied" : "Copy details"}</Button>
    {/if}
    {#if item.action}
      <Button size="sm" variant="ghost" aria-label={actionName} onclick={runAction}>
        {item.action.label}
      </Button>
    {/if}
    <IconButton icon="close" label="Dismiss" size="sm" onclick={() => !$modalOpen && dismissToast(item.id)} />
  </span>
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
  .controls {
    display: contents;
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
  .text {
    flex: 1;
    min-width: 0;
  }
  .message {
    margin: 0;
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
    overflow-wrap: anywhere;
  }
  .body {
    margin: var(--sp-1) 0 0;
    color: var(--muted);
    font-size: var(--fs-xs);
    line-height: var(--lh-sm);
    overflow-wrap: anywhere;
    /* Several reasons arrive one per line. */
    white-space: pre-line;
  }

  .count {
    margin-left: var(--sp-1);
    padding: 0 var(--sp-1);
    border-radius: var(--r-pill);
    background: var(--sunken);
    color: var(--muted);
    font-size: var(--fs-xs);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
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
