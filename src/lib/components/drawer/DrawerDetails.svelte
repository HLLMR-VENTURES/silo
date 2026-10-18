<script lang="ts">
  // Details: the facts from the mod's files, as label / value pairs. Mod ID (the techName) has a
  // copy button; dependencies say whether each is in the library.
  import Icon from "../../ui/Icon.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import { toast } from "../../ui/toasts.svelte";
  import { copyText } from "../../api";
  import type { LogModHealth, ModEntry } from "../../types";
  import { plural, formatCount } from "../../plural";
  import { formatSize } from "./drawerText";

  interface Props {
    mod: ModEntry;
    organized: boolean;
    libraryTechNames: Set<string>;
    /** The last game log's entry for the mod: null when the log didn't name it, undefined when
     *  there's no log to read. */
    lastRun: LogModHealth | null | undefined;
  }

  let { mod, organized, libraryTechNames, lastRun }: Props = $props();
  const uid = $props.id();

  const type = $derived(mod.isMap ? "Map" : mod.kind === "dir" ? "Folder mod" : "Zip");

  const lastRunText = $derived.by(() => {
    if (lastRun === undefined) return null;
    if (lastRun === null) return "Nothing in the last game's log";
    if (lastRun.errors > 0) return `${formatCount(lastRun.errors)} ${plural(lastRun.errors, { one: "error", other: "errors" })}`;
    const real = lastRun.warnings - lastRun.benign;
    if (real > 0) return `${formatCount(real)} ${plural(real, { one: "warning", other: "warnings" })}`;
    if (lastRun.benign > 0) return "Cosmetic warnings only";
    return "No errors or warnings";
  });

  // The log's sample line for warnings (errors have their own banner with it).
  const lastRunSample = $derived(
    lastRun && lastRun.errors === 0 && lastRun.warnings - lastRun.benign > 0 ? lastRun.sample : "",
  );

  async function copyId() {
    try {
      await copyText(mod.techName);
      toast({ message: "Mod ID copied", tone: "success" });
    } catch {
      toast({ message: "Silo couldn't copy the Mod ID.", tone: "warn" });
    }
  }
</script>

<section class="sec" aria-labelledby="{uid}-details">
  <h3 id="{uid}-details" class="h">Details</h3>
  <dl class="kv">
    <div><dt>Category</dt><dd>{mod.category}{mod.subcategory ? ` · ${mod.subcategory}` : ""}</dd></div>
    <div><dt>Size</dt><dd class="tnum">{formatSize(mod.size)}</dd></div>
    <div><dt>Type</dt><dd>{type}</dd></div>
    {#if mod.storeItemCount > 0}<div><dt>Shop items</dt><dd class="tnum">{mod.storeItemCount}</dd></div>{/if}
    {#if mod.scriptCount > 0}<div><dt>Scripts</dt><dd class="tnum">{mod.scriptCount}</dd></div>{/if}
    <div><dt>Multiplayer</dt><dd>{mod.mpSupported ? "Supported" : "Not marked"}</dd></div>
    {#if organized}<div><dt>Library</dt><dd>Organized</dd></div>{/if}
    {#if lastRunText}<div><dt>Last run</dt><dd>{lastRunText}</dd></div>{/if}
    {#if lastRunSample}<p class="sample">{lastRunSample}</p>{/if}
    <div class="id">
      <dt>Mod ID</dt>
      <dd>
        <code class="mono">{mod.techName}</code>
        <IconButton icon="copy" size="sm" label="Copy Mod ID" onclick={copyId} />
      </dd>
    </div>
    {#if mod.uniqueType}
      <div><dt>Exclusive slot</dt><dd><code class="mono">{mod.uniqueType}</code></dd></div>
    {/if}
  </dl>

  {#if mod.dependencies.length > 0}
    <h4 class="sub">Needs {formatCount(mod.dependencies.length)} {plural(mod.dependencies.length, { one: "mod", other: "mods" })}</h4>
    <ul class="deps">
      {#each mod.dependencies as dep (dep)}
        <li>
          <code class="mono">{dep}</code>
          {#if libraryTechNames.has(dep)}
            <span class="ok"><Icon name="ok" size={12} /> In your library</span>
          {:else}
            <span class="missing"><Icon name="problem" size={12} /> Missing</span>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .sec {
    padding: var(--sp-3) 0;
    border-top: 1px solid var(--border);
  }
  .h {
    margin: 0 0 var(--sp-2);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
  }
  .kv {
    display: grid;
    gap: 6px;
    margin: 0;
    font-size: var(--fs-xs);
  }
  .kv > div {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
  }
  dt {
    color: var(--text-muted);
    flex: none;
  }
  dd {
    margin: 0;
    text-align: right;
    min-width: 0;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    overflow-wrap: anywhere;
  }
  .sample {
    margin: 0;
    font-family: var(--font-mono);
    color: var(--text-muted);
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }
  .sub {
    margin: var(--sp-3) 0 var(--sp-1);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text);
  }
  .deps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: var(--fs-xs);
  }
  .deps li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
  }
  .ok {
    color: var(--primary-text);
  }
  .missing {
    color: var(--warn-text);
    font-weight: 600;
  }
</style>
