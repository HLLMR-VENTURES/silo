<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import type { PlannedMove } from "../types";

  let {
    plan,
    loading,
    applying,
    onConfirm,
    onCancel,
    adopt = null,
  }: {
    plan: PlannedMove[];
    loading: boolean;
    applying: boolean;
    onConfirm: (skipNext: boolean) => void;
    onCancel: () => void;
    /** Adopting loose mods already sitting in a custom library archive (#166): `path` is that
     *  folder, `note` an info line (e.g. duplicates left untouched). */
    adopt?: { path: string | null; note: string | null } | null;
  } = $props();

  // plan_organize's relTo is `archive/<Category>/<file>` — right for the default archive, but an
  // adopted mod lands in a custom archive folder, so show the path inside that folder instead.
  const dest = (mv: PlannedMove) => (adopt ? mv.relTo.replace(/^archive\//, "") : mv.relTo);
  const noun = (n: number) => `${n} mod${n === 1 ? "" : "s"}`;

  let skipNext = $state(false);

  // Group the planned moves by destination category so the user sees the shape of the
  // operation ("12 into Vehicles, 3 into Maps") before any file is touched.
  const groups = $derived.by(() => {
    const m = new Map<string, PlannedMove[]>();
    for (const p of plan) {
      const arr = m.get(p.category) ?? [];
      arr.push(p);
      m.set(p.category, arr);
    }
    return [...m.entries()]
      .map(([category, moves]) => ({ category, moves }))
      .sort((a, b) => a.category.localeCompare(b.category));
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={onCancel}></div>
<div
  class="modal"
  role="dialog"
  aria-modal="true"
  aria-label={adopt ? "File mods into your library" : "Organize preview"}
>
  <div class="modal-head">
    <h3>{adopt ? `File ${noun(plan.length)} into your library` : `Organize ${noun(plan.length)}`}</h3>
    <button class="drawer-x" title="Close" aria-label="Close" onclick={onCancel}><Icon name="close" /></button>
  </div>

  <div class="modal-body">
    {#if loading}
      <p class="muted">Working out what would move…</p>
    {:else if plan.length === 0}
      <p class="muted">Nothing to organize — every loose mod is already filed.</p>
    {:else if adopt}
      <p class="lead">
        Your library archive{#if adopt.path}{" "}(<code>{adopt.path}</code>){/if} already holds loose
        mod zips. Silo can file them into its category folders there so they show up in your
        library (parked — activate them as usual). Nothing is overwritten, and nothing has moved yet.
      </p>
      {#if adopt.note}<p class="muted">{adopt.note}</p>{/if}
    {:else}
      <p class="lead">
        This files your loose mod zips into <code>mods/archive/&lt;Category&gt;/</code>.
        Active mods stay loaded: Silo links them back into your mods folder, so in most
        setups they take no extra disk space. Nothing has moved yet, and
        <b>Undo organizing</b> in Settings → Maintenance puts every mod back.
      </p>

      <div class="groups">
        {#each groups as g (g.category)}
          <div class="grp">
            <div class="grp-head">
              <span class="grp-name">{g.category}</span>
              <span class="grp-count tnum">{g.moves.length}</span>
            </div>
            <ul class="files">
              {#each g.moves as mv (mv.techName)}
                <li title={dest(mv)}>
                  <span class="fn">{mv.fileName}</span>
                  <span class="arrow">→</span>
                  <span class="dest mono">{dest(mv)}</span>
                </li>
              {/each}
            </ul>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="modal-foot">
    {#if adopt}
      <span></span>
    {:else}
      <label class="skip">
        <input type="checkbox" bind:checked={skipNext} disabled={loading || applying} />
        Don't preview next time
      </label>
    {/if}
    <div class="acts">
      <button class="btn ghost" onclick={onCancel} disabled={applying}>Cancel</button>
      <button
        class="btn primary"
        onclick={() => onConfirm(skipNext)}
        disabled={loading || applying || plan.length === 0}
      >
        {#if adopt}
          {applying ? "Filing…" : `File ${noun(plan.length)} into your library`}
        {:else}
          {applying ? "Organizing…" : `Organize ${noun(plan.length)}`}
        {/if}
      </button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 50;
  }
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(640px, 92vw);
    max-height: 82vh;
    display: flex;
    flex-direction: column;
    background: var(--surface, var(--bg));
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    z-index: 51;
  }
  .modal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .modal-head h3 {
    margin: 0;
    font-size: 1rem;
    font-family: var(--font-display);
  }
  .drawer-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 1rem;
    cursor: pointer;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .drawer-x:hover {
    background: var(--bg);
    color: var(--text);
  }
  .modal-body {
    overflow-y: auto;
    padding: 16px;
  }
  .lead {
    margin: 0 0 14px;
    color: var(--text);
    font-size: 0.85rem;
    line-height: 1.55;
  }
  .lead code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    background: var(--bg);
    padding: 1px 4px;
    border-radius: 4px;
  }
  .muted {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.88rem;
  }
  .groups {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .grp {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised, var(--bg));
    overflow: hidden;
  }
  .grp-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 11px;
    border-bottom: 1px solid var(--border);
  }
  .grp-name {
    font-weight: 600;
    font-size: var(--fs-sm);
    color: var(--text);
  }
  .grp-count {
    font-size: var(--fs-xs);
    color: var(--on-primary);
    background: var(--primary);
    border-radius: 999px;
    padding: 1px 9px;
    font-weight: 600;
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    max-height: 168px;
    overflow-y: auto;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 3px 11px;
    font-size: var(--fs-xs);
    color: var(--text);
  }
  .fn {
    flex: 0 1 auto;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .arrow {
    color: var(--text-muted);
    flex: 0 0 auto;
  }
  .dest {
    flex: 1 1 auto;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
  }
  .modal-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  .skip {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    cursor: pointer;
  }
  .acts {
    display: flex;
    gap: 8px;
  }
  .btn {
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .btn.ghost:hover:not(:disabled) {
    background: var(--bg);
  }
  .btn.primary {
    background: var(--primary);
    border-color: transparent;
    color: var(--on-primary);
    font-weight: 600;
  }
  .btn.primary:hover:not(:disabled) {
    filter: brightness(1.06);
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
