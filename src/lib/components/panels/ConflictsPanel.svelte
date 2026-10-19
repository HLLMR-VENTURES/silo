<script lang="ts">
  // Diagnose → Conflicts: what clashes in the active set, in plain words.
  //   <ConflictsPanel {conflicts} activeCount={142} onOpen={openDrawer} onPark={park} />
  // Headline "0 problems · 12 to review · 19 notes hidden"; each row has a Problem / Review /
  // Note label, a plain title, the mods as links (they open the drawer), the explanation, and a
  // "Park one…" menu. Notes stay hidden until "Show notes" is on.
  import Badge from "../../ui/Badge.svelte";
  import Button from "../../ui/Button.svelte";
  import MenuButton from "../../ui/MenuButton.svelte";
  import Switch from "../../ui/Switch.svelte";
  import {
    canParkOne,
    conflictCounts,
    conflictHeadline,
    conflictTitle,
    severityLabel,
  } from "../../diagnose/conflictCopy";
  import { formatCount, plural } from "../../plural";
  import type { Conflict } from "../../types";

  interface Props {
    conflicts: Conflict[];
    activeCount: number;
    /** Open a mod's drawer (by tech name). */
    onOpen?: (techName: string) => void;
    /** Park a mod (by tech name), with Undo. */
    onPark?: (techName: string) => void;
    /** Show notes too (bindable; off by default). */
    showNotes?: boolean;
    /** Why library writes are off right now (Park one… is off with this reason), or "". */
    lockReason?: string;
  }

  let { conflicts, activeCount, onOpen, onPark, showNotes = $bindable(false), lockReason = "" }: Props = $props();

  const counts = $derived(conflictCounts(conflicts));
  const headline = $derived(conflictHeadline(counts, showNotes));
  const shown = $derived(showNotes ? conflicts : conflicts.filter((c) => c.severity !== "info"));

  const tone = (s: Conflict["severity"]) => (s === "critical" ? "danger" : s === "warning" ? "warn" : "info");
</script>

<section class="conflicts-panel" aria-labelledby="conflicts-head">
  <div class="lp-head">
    <h3 id="conflicts-head" class="lp-title tnum">
<strong>{headline[0]}</strong><span class="part">{headline
          .slice(1)
          .map((p) => ` · ${p}`)
          .join("")}</span>
    </h3>
    {#if counts.notes > 0}
      <Switch label="Show notes" bind:checked={showNotes} />
    {/if}
  </div>

  {#if conflicts.length === 0}
    <p class="lp-empty">
      {#if activeCount === 0}
        No mods are active, so nothing can conflict. Silo checks again whenever you change which mods are active.
      {:else}
        No conflicts among your {formatCount(activeCount)} active {plural(activeCount, { one: "mod", other: "mods" })}. Silo
        checks again whenever you change which mods are active.
      {/if}
    </p>
  {:else if shown.length === 0}
    <p class="lp-empty">No problems and nothing to review. Notes are only for your information.</p>
  {/if}

  <ul class="rows">
    {#each shown as c (c.id)}
      <li class="cf-row sev-{c.severity}">
        <div class="cf-top">
          <Badge tone={tone(c.severity)}>{severityLabel(c.severity)}</Badge>
          <h4 class="cf-title">{conflictTitle(c)}</h4>
          {#if c.kind !== "map"}<code class="cf-name">{c.name}</code>{/if}
        </div>
        <p class="cf-mods">
          {#each c.mods as label, i (c.techNames[i] ?? label)}
            {#if i > 0}<span class="sep" aria-hidden="true"> · </span>{/if}
            {#if onOpen && c.techNames[i]}
              <Button variant="link" size="sm" onclick={() => onOpen(c.techNames[i])}>{label}</Button>
            {:else}
              <span>{label}</span>
            {/if}
          {/each}
        </p>
        <p class="cf-why">{c.explanation}</p>
        {#if onPark && canParkOne(c)}
          <div class="cf-actions">
            <MenuButton
              label="Park one…"
              size="sm"
              items={c.techNames.map((t, i) => ({ id: t, label: `Park ${c.mods[i] ?? t}` }))}
              onselect={(t) => onPark(t)}
              disabled={!!lockReason}
            />
            {#if lockReason}<span class="why">{lockReason}</span>{/if}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
</section>

<style>
  /* A page section in its view. */
  .conflicts-panel {
    max-width: 48rem;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
  }
  .lp-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    padding: 6px 8px 10px;
  }
  .lp-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1rem;
    font-weight: 500;
  }
  .lp-title .part {
    color: var(--text-muted);
  }
  .sep {
    color: var(--text-muted);
  }
  .lp-empty {
    margin: 0;
    padding: 10px 8px 14px;
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .cf-row {
    padding: 10px 10px 12px;
    border-radius: var(--radius-sm);
    border-left: 3px solid var(--warn-text);
    background: color-mix(in srgb, var(--warn-text) 6%, transparent);
    margin-bottom: 8px;
  }
  .cf-row.sev-critical {
    border-left-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 6%, transparent);
  }
  .cf-row.sev-info {
    border-left-color: var(--info);
    background: color-mix(in srgb, var(--info) 5%, transparent);
  }
  .cf-top {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .cf-title {
    margin: 0;
    font-weight: 600;
    font-size: var(--fs-md, 0.95rem);
    font-family: var(--font-display);
  }
  .cf-name {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .cf-mods {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px 4px;
    margin: 6px 0 0;
    font-size: var(--fs-sm);
  }
  .cf-why {
    margin: 5px 0 0;
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .cf-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
  }
  .why {
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }
</style>
