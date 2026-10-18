<script lang="ts">
  // Modal dialog:
  //   <Dialog bind:open title="Rename mod set" onclose={(why) => ...}>
  //     ...body...
  //     {#snippet footer()}<Button onclick={...}>Cancel</Button><Button variant="primary">Save</Button>{/snippet}
  //   </Dialog>
  // Portals to <body>, makes the rest of the page inert (see overlayStack.ts), traps Tab, closes
  // on Esc / scrim click when `dismissible`, and returns focus to whatever had it before.
  // Initial focus: `initialFocus` if given; otherwise an element marked `data-autofocus`, else the
  // first focusable element in the body, else the first footer button. Text inputs get selected.
  // For a quick yes/no or text prompt use `ask()` / `askText()` from dialogs.svelte.ts instead.
  import { tick, untrack, type Snippet } from "svelte";
  import { portal } from "./portal";
  import { focusableIn, pushLayer } from "./overlayStack";

  type CloseReason = "escape" | "scrim" | "programmatic";

  interface Props {
    open?: boolean;
    title: string;
    description?: string | Snippet;
    size?: "sm" | "md" | "lg";
    initialFocus?: HTMLElement | "first" | "none";
    /** Called once per close. "programmatic" = the parent set `open` false (or unmounted the dialog). */
    onclose?: (reason: CloseReason) => void;
    /** Esc and scrim click close the dialog. */
    dismissible?: boolean;
    children?: Snippet;
    footer?: Snippet;
    class?: string;
  }

  let {
    open = $bindable(false),
    title,
    description,
    size = "md",
    initialFocus,
    onclose,
    dismissible = true,
    children,
    footer,
    class: className = "",
  }: Props = $props();

  const uid = $props.id();
  const titleId = `${uid}-title`;
  const descId = `${uid}-desc`;

  let host = $state<HTMLElement | null>(null);
  let panel = $state<HTMLElement | null>(null);
  let bodyEl = $state<HTMLElement | null>(null);
  let footerEl = $state<HTMLElement | null>(null);
  /** Set when the dialog closes itself, so the effect teardown doesn't report "programmatic" too. */
  let closedInternally = false;

  function close(reason: CloseReason) {
    if (!open) return;
    closedInternally = true;
    open = false;
    onclose?.(reason);
  }

  function focusTarget(): HTMLElement | null {
    const want = untrack(() => initialFocus);
    if (want === "none") return panel;
    if (want instanceof HTMLElement) return want;
    if (want !== "first") {
      const marked = panel?.querySelector<HTMLElement>("[data-autofocus]");
      if (marked) return marked;
    }
    const inBody = bodyEl ? focusableIn(bodyEl)[0] : undefined;
    const inFooter = footerEl ? focusableIn(footerEl)[0] : undefined;
    return inBody ?? inFooter ?? panel;
  }

  $effect(() => {
    if (!open || !host) return;
    const layerHost = host;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closedInternally = false;
    const layer = pushLayer({
      host: layerHost,
      modal: true,
      onEscape: () => {
        if (untrack(() => dismissible)) close("escape");
      },
    });
    tick().then(() => {
      if (!layerHost.isConnected) return;
      const el = focusTarget();
      el?.focus();
      if (el instanceof HTMLInputElement && /^(text|search|url|email|tel|)$/.test(el.type)) el.select();
    });
    return () => {
      layer.pop();
      if (!closedInternally) untrack(() => onclose)?.("programmatic");
      closedInternally = false;
      if (opener?.isConnected) opener.focus();
    };
  });

  function trapTab(e: KeyboardEvent) {
    if (e.key !== "Tab" || !panel) return;
    const items = focusableIn(panel);
    if (items.length === 0) {
      e.preventDefault();
      panel.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    if (e.shiftKey && (active === first || active === panel)) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && active === last) {
      e.preventDefault();
      first.focus();
    }
  }

  function onScrim() {
    if (dismissible) close("scrim");
  }
</script>

{#if open}
  <div class="dialog-layer" use:portal bind:this={host}>
    <!-- Pointer-only affordance; Esc is the keyboard equivalent. -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="scrim" onclick={onScrim}></div>
    <div
      bind:this={panel}
      class="panel size-{size} {className}"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
      aria-describedby={description ? descId : undefined}
      tabindex="-1"
      onkeydown={trapTab}
    >
      <h2 class="title" id={titleId}>{title}</h2>
      {#if description}
        <div class="description" id={descId}>
          {#if typeof description === "string"}{description}{:else}{@render description()}{/if}
        </div>
      {/if}
      <div class="body" bind:this={bodyEl}>
        {@render children?.()}
      </div>
      {#if footer}
        <div class="footer" bind:this={footerEl}>
          {@render footer()}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* Each dialog layer is its own stacking context (above --z-scrim, so the scrim inside it covers
     drawers and popovers); a nested dialog's layer comes later in <body> and covers the one below. */
  .dialog-layer {
    position: fixed;
    inset: 0;
    z-index: var(--z-dialog);
    display: grid;
    place-items: center;
    padding: var(--sp-5);
  }
  .scrim {
    position: absolute;
    inset: 0;
    background: var(--scrim);
    animation: fade var(--dur-2) var(--ease);
  }
  .panel {
    position: relative;
    display: flex;
    flex-direction: column;
    width: 100%;
    max-height: calc(100vh - 2 * var(--sp-6));
    padding: var(--sp-5);
    border-radius: var(--r-lg);
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--e3);
    animation: rise var(--dur-2) var(--ease);
  }
  .panel:focus {
    outline: none;
  }
  .size-sm {
    max-width: 26rem;
  }
  .size-md {
    max-width: 34rem;
  }
  .size-lg {
    max-width: 48rem;
  }
  .title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    line-height: var(--lh-xl);
    font-weight: 600;
  }
  .description {
    margin-top: var(--sp-2);
    color: var(--muted);
  }
  .body {
    min-height: 0;
    margin-top: var(--sp-4);
    overflow: auto;
  }
  .body:empty {
    display: none;
  }
  .footer {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: var(--sp-2);
    margin-top: var(--sp-5);
  }

  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: scale(0.98);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panel {
      animation-name: fade;
    }
  }
</style>
