<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import type { ModEntry, LogModHealth, CatalogUpdate } from "../types";
  import { scanLog, catalogCheckUpdates, openExternal } from "../api";

  let { mod }: { mod: ModEntry } = $props();

  // Per-mod status pulled in on open: how it fared in the last run's log, and what the
  // catalog knows about it. Both best-effort — a missing log or offline catalog just
  // hides the section rather than erroring.
  let logHealth = $state<LogModHealth | null | "clean">(null); // null=unknown, "clean"=in log, no findings
  let catalog = $state<CatalogUpdate | null>(null);
  let catalogChecked = $state(false);

  $effect(() => {
    const tech = mod.techName; // re-run when the drawer switches mods
    logHealth = null;
    catalog = null;
    catalogChecked = false;
    scanLog()
      .then((r) => {
        const m = r.mods.find((x) => x.modName === tech);
        logHealth = m ?? (r.mods.length >= 0 ? "clean" : null);
      })
      .catch(() => {});
    catalogCheckUpdates([{ techName: tech, version: mod.version ?? undefined }])
      .then((rows) => {
        catalog = rows.find((x) => x.techName === tech) ?? null;
        catalogChecked = true;
      })
      .catch(() => {});
  });
</script>

{#if logHealth}
  <div class="d-section">
    <div class="d-label">Last run</div>
    {#if logHealth === "clean"}
      <div class="d-log ok">No errors or warnings from this mod in the last game log.</div>
    {:else if logHealth.errors > 0}
      <div class="d-log bad">
        {logHealth.errors} error{logHealth.errors === 1 ? "" : "s"} in the last run —
        likely the reason something's broken.
        <div class="d-log-sample">{logHealth.sample}</div>
      </div>
    {:else if logHealth.warnings - logHealth.benign > 0}
      <div class="d-log warn">
        {logHealth.warnings - logHealth.benign} warning{logHealth.warnings - logHealth.benign === 1 ? "" : "s"} last run.
        <div class="d-log-sample">{logHealth.sample}</div>
      </div>
    {:else}
      <div class="d-log ok">{logHealth.benign} cosmetic warning{logHealth.benign === 1 ? "" : "s"} only — safe to ignore.</div>
    {/if}
  </div>
{/if}

<div class="d-section">
  <div class="d-label">Catalog</div>
  {#if !catalogChecked}
    <div class="d-log muted">Checking the catalog…</div>
  {:else if !catalog}
    <div class="d-log muted">Not found in the SiloAPI catalog.</div>
  {:else if catalog.hasUpdate}
    {@const c = catalog}
    <div class="d-log warn">
      Update available: you have {mod.version ?? "?"}, catalog has <b>{c.latest}</b>{c.source ? ` (${c.source})` : ""}.
      {#if c.downloadUrl}<button class="d-cat-link" onclick={() => openExternal(c.downloadUrl!)}>Get it <Icon name="external" size={12} /></button>{/if}
    </div>
  {:else}
    <div class="d-log ok">Up to date with the catalog{catalog.latest ? ` (${catalog.latest})` : ""}.</div>
  {/if}
</div>

<style>
  .d-section {
    padding: 12px 0;
    border-top: 1px solid var(--border);
  }
  .d-label {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
    margin-bottom: 8px;
  }
  .d-log {
    font-size: 0.85rem;
    line-height: 1.5;
    color: var(--text);
  }
  .d-log.ok {
    color: var(--text-muted);
  }
  .d-log.muted {
    color: var(--text-muted);
    opacity: 0.85;
  }
  .d-log.warn {
    color: var(--warn-text);
  }
  .d-log.bad {
    color: var(--danger);
  }
  .d-log-sample {
    margin-top: 4px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .d-cat-link {
    border: none;
    background: transparent;
    color: var(--info);
    font: inherit;
    cursor: pointer;
    padding: 0;
    margin-left: 4px;
    text-decoration: underline;
  }
</style>
