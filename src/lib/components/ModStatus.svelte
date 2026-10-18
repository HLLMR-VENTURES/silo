<script lang="ts">
  // What Silo's catalog says about this mod's version, inside the drawer's "Source and updates".
  // The drawer does the (best-effort) lookup and passes the result: `checked` false while it
  // runs, `catalog` null when the mod isn't in the catalog or the lookup failed.
  import Icon from "../ui/Icon.svelte";
  import type { ModEntry, CatalogUpdate } from "../types";
  import { openExternal } from "../api";

  let { mod, catalog, checked }: { mod: ModEntry; catalog: CatalogUpdate | null; checked: boolean } = $props();
</script>

<div class="d-section">
  <h4 class="d-label">Silo's catalog</h4>
  {#if !checked}
    <div class="d-log muted">Checking the catalog…</div>
  {:else if !catalog}
    <div class="d-log muted">Not in Silo's catalog.</div>
  {:else if catalog.hasUpdate}
    {@const c = catalog}
    <div class="d-log warn">
      Update available: you have {mod.version ?? "an unknown version"}, the catalog has <b>{c.latest}</b>{c.source
        ? ` (${c.source})`
        : ""}.
      {#if c.downloadUrl}<button class="d-cat-link" onclick={() => openExternal(c.downloadUrl!)}
          >Get it <Icon name="external" size={12} /><span class="vh"> (opens in your browser)</span></button
        >{/if}
    </div>
  {:else}
    <div class="d-log ok">Up to date with the catalog{catalog.latest ? ` (${catalog.latest})` : ""}.</div>
  {/if}
</div>

<style>
  .d-section {
    padding: 6px 0;
  }
  .d-label {
    margin: 0 0 4px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text);
  }
  .d-log {
    font-size: var(--fs-xs);
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
  .vh {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
