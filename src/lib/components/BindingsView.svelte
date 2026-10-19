<script lang="ts">
  // The complete control-binding map the in-game help never shows: every action↔input
  // for every device, searchable, with inputs that drive multiple actions highlighted
  // for review. A view, not a verdict — reused inputs are usually legitimate context
  // bindings, so we surface them and let the player decide.
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { describeError } from "../errorCopy";
  import { onMount } from "svelte";
  import { scanBindings } from "../api";
  import type { BindingReport } from "../types";
  import { tooltip } from "../ui/tooltip";

  interface Props {
    /** Shows a close button (the page has none). */
    onClose?: () => void;
  }
  let { onClose }: Props = $props();

  let report = $state<BindingReport | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  /** FS25 hasn't written its controls file yet (never played): an empty state, not an error. */
  let noFile = $state(false);
  let query = $state("");

  // Friendly device names for the common FS device ids.
  function deviceLabel(id: string): string {
    if (id === "KB_MOUSE_DEFAULT") return "Keyboard and mouse";
    if (id === "GAMEPAD" || id.startsWith("GAMEPAD")) return "Gamepad";
    return id || "Unknown device";
  }

  // Strip the FS input prefixes so the map reads like a human wrote it.
  function prettyInput(s: string): string {
    return s
      .split(" ")
      .map((tok) => tok.replace(/^KEY_/, "").replace(/^MOUSE_BUTTON_/, "Mouse ").replace(/^BUTTON_/, "Btn "))
      .join(" + ");
  }
  // "AXIS_MOVE_SIDE_VEHICLE" -> "Axis move side vehicle": sentence case, like the rest of Silo.
  function prettyAction(s: string): string {
    const t = s.replace(/_/g, " ").toLowerCase();
    return t.charAt(0).toUpperCase() + t.slice(1);
  }

  function match(q: string, ...fields: string[]): boolean {
    if (!q) return true;
    const n = q.toLowerCase();
    return fields.some((f) => f.toLowerCase().includes(n));
  }

  async function load() {
    loading = true;
    error = null;
    noFile = false;
    try {
      report = await scanBindings();
    } catch (e) {
      if (describeError(e).code === "NO_LOG") noFile = true;
      else error = inlineError(e, "Reading your key bindings");
      report = null;
    } finally {
      loading = false;
    }
  }

  onMount(load);
</script>

<div class="bv">
  <div class="bv-head">
    <div>
      <h3>Control bindings</h3>
      {#if report}
        <p class="sub">
          {report.totalActions.toLocaleString()} actions · {report.totalBindings.toLocaleString()} bindings
          · {report.devices.length} device{report.devices.length === 1 ? "" : "s"}
        </p>
      {/if}
    </div>
    <div class="bv-head-actions">
      {#if onClose}
        <button class="x" use:tooltip={"Close"} aria-label="Close" onclick={onClose}><Icon name="close" /></button>
      {/if}
    </div>
  </div>

  {#if loading}
    <div class="empty">Reading your control settings…</div>
  {:else if noFile}
    <div class="empty">No controls file yet. Play once, then come back here.</div>
  {:else if error}
    <div class="err">{error}</div>
  {:else if report}
    {@const r = report}
    <input
      class="search"
      type="search"
      aria-label="Search actions or keys"
      placeholder="Search actions or keys…"
      bind:value={query}
    />

    {#each r.devices as d (d.device)}
      {@const shared = d.shared.filter((s) => match(query, s.input, ...s.actions))}
      {@const binds = d.bindings.filter((b) => match(query, b.action, b.input))}
      <div class="device">
        <div class="device-name">{deviceLabel(d.device)}</div>

        {#if shared.length > 0}
          <div class="sec">
            Inputs bound to more than one action
            <span class="sec-note">Usually intentional (different contexts). Review if a control feels double-booked.</span>
          </div>
          {#each shared as s (s.input)}
            <div class="shared-row">
              <span class="key">{prettyInput(s.input)}</span>
              <span class="arrow">→</span>
              <span class="acts">
                {#each s.actions as a, i (a)}<span class="act">{prettyAction(a)}</span>{#if i < s.actions.length - 1}<span class="sep">·</span>{/if}{/each}
              </span>
            </div>
          {/each}
        {/if}

        {#if binds.length > 0}
          <div class="sec">All bindings ({binds.length})</div>
          <div class="grid">
            {#each binds as b (b.action + b.input)}
              <div class="bind">
                <span class="b-action">{prettyAction(b.action)}</span>
                <span class="b-key">{prettyInput(b.input)}</span>
              </div>
            {/each}
          </div>
        {:else if query}
          <div class="empty small">No bindings match “{query}” on this device.</div>
        {/if}
      </div>
    {/each}

    {#if r.devices.length === 0}
      <div class="empty">No bindings found. Set up your controls in Farming Simulator 25 first.</div>
    {/if}
  {/if}
</div>

<style>
  .bv {
    padding: 16px 18px 22px;
  }
  .bv-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .bv-head h3 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.35rem;
    color: var(--text);
  }
  .sub {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 1rem;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .x:hover {
    background: var(--bg);
    color: var(--text);
  }
  .search {
    width: 100%;
    padding: 9px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    margin-bottom: 14px;
  }
  .empty {
    color: var(--text-muted);
    text-align: center;
    padding: 40px 0;
  }
  .empty.small {
    padding: 12px 0;
    font-size: 0.85rem;
  }
  .err {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
  }
  .device {
    margin-bottom: 18px;
  }
  .device-name {
    font-weight: 600;
    color: var(--text);
    font-size: 1rem;
    padding-bottom: 4px;
    border-bottom: 2px solid var(--border);
    margin-bottom: 8px;
  }
  .sec {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin: 12px 0 6px;
  }
  .sec-note {
    text-transform: none;
    letter-spacing: 0;
  }
  .shared-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    margin-bottom: 4px;
    font-size: 0.85rem;
  }
  .arrow {
    color: var(--text-muted);
  }
  .acts {
    color: var(--text);
  }
  .sep {
    color: var(--text-muted);
    margin: 0 5px;
  }
  .key,
  .b-key {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-sm);
    color: var(--primary-text);
    white-space: nowrap;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 2px;
  }
  .bind {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }
  .bind:nth-child(odd) {
    background: var(--bg);
  }
  .b-action {
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
