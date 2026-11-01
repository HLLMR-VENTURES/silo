<script lang="ts">
  // What Silo's catalog says about this mod's version, inside the drawer's "Source and updates".
  // The drawer does the (best-effort) lookup and passes the result: `checked` false while it
  // runs, `catalog` null when the mod isn't in the catalog, `failed` when the lookup itself failed.
  import Icon from "../ui/Icon.svelte";
  import { displayVersion } from "../format";
  import type { ModEntry, CatalogUpdate } from "../types";
  import { openExternal } from "../api";

  let {
    mod,
    catalog,
    checked,
    failed = false,
    pageUrl = null,
  }: {
    mod: ModEntry;
    catalog: CatalogUpdate | null;
    checked: boolean;
    /** The catalog request failed, so Silo can't say whether the mod is in it. */
    failed?: boolean;
    /** The catalog's page for the mod, offered when there's no download link. */
    pageUrl?: string | null;
  } = $props();
  const link = $derived(catalog?.downloadUrl ?? pageUrl);
</script>

<div class="d-section">
  <h4 class="d-label">Silo's catalog</h4>
  {#if !checked}
    <div class="d-log muted">Checking the catalog…</div>
  {:else if failed}
    <div class="d-log warn">Silo couldn't reach its catalog, so it can't say whether this mod is in it. Try again later.</div>
  {:else if !catalog}
    <div class="d-log muted">Not in Silo's catalog.</div>
  {:else if catalog.error}
    <div class="d-log warn">{catalog.error}</div>
  {:else if catalog.hasUpdate}
    {@const c = catalog}
    <div class="d-log warn">
      Update available: you have {mod.version ? displayVersion(mod.version) : "an unknown version"}, and Silo's catalog has <b>{displayVersion(c.latest)}</b>{c.source
        ? ` (${c.source === "github" ? "GitHub" : c.source === "modhub" ? "ModHub" : c.source})`
        : ""}.
      {#if link}<button class="d-cat-link" onclick={() => openExternal(link)}
          >{c.downloadUrl ? "Get it" : "Open its page"} <Icon name="external" size={12} /><span class="vh"> (opens in your browser)</span></button
        >{/if}
    </div>
  {:else}
    <div class="d-log ok">Up to date with the catalog{catalog.latest ? ` (${displayVersion(catalog.latest)})` : ""}.</div>
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
