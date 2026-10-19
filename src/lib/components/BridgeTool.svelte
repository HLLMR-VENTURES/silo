<script lang="ts">
  // Filltype compatibility bridge: generate a companion mod that adds a custom fill type
  // into the categories your equipment accepts — the "nothing can haul my sugar beets" fix.
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { generateBridge } from "../api";
  import { blocked, tryStart } from "../state/ops.svelte";
  import { tooltip } from "../ui/tooltip";

  interface Props {
    onGenerated: (filename: string) => void; // parent rescans
    /** Shows the close buttons (the page has none). */
    onClose?: () => void;
  }
  let { onGenerated, onClose }: Props = $props();

  // The handling categories worth bridging into, grouped so the choice reads plainly.
  // Names are the game's category keys (verified against base maps_fillTypes.xml).
  const CATEGORY_GROUPS: { label: string; cats: { id: string; label: string }[] }[] = [
    {
      label: "Haul & load",
      cats: [
        { id: "BULK", label: "Trailers (BULK)" },
        { id: "TIPPER", label: "Tippers (TIPPER)" },
        { id: "SHOVEL", label: "Front loaders (SHOVEL)" },
        { id: "AUGERWAGON", label: "Auger wagons" },
        { id: "TRAINWAGON", label: "Train wagons" },
        { id: "LOADINGVEHICLE", label: "Loading vehicles" },
      ],
    },
    {
      label: "Store & sell",
      cats: [
        { id: "FARMSILO", label: "Farm silos" },
        { id: "SELLINGSTATION_BULKMATERIAL", label: "Bulk sell points" },
      ],
    },
    {
      label: "Forage & feed",
      cats: [
        { id: "SILAGETRAILER", label: "Silage trailers" },
        { id: "FORAGEWAGON", label: "Forage wagons" },
        { id: "MIXERWAGON", label: "Mixer wagons" },
      ],
    },
  ];
  // Sensible default: make it haulable, loadable, storable, sellable.
  const DEFAULT_CATS = ["BULK", "TIPPER", "SHOVEL", "FARMSILO", "SELLINGSTATION_BULKMATERIAL"];

  let fillTypesRaw = $state("");
  let selected = $state<Set<string>>(new Set(DEFAULT_CATS));
  let name = $state("FS25_SiloFilltypeBridge");
  /** Generating right now (the button is off meanwhile). */
  let generating = $state(false);
  let error = $state<string | null>(null);
  let done = $state<string | null>(null);

  // Once each: "sugarbeet SUGARBEET" is one fill type (and the chips are keyed by name).
  const fillTypeList = $derived([
    ...new Set(
      fillTypesRaw
        .split(/[\s,]+/)
        .map((s) => s.trim().toUpperCase())
        .filter(Boolean),
    ),
  ]);
  const canGenerate = $derived(fillTypeList.length > 0 && selected.size > 0 && !generating);

  function toggle(id: string) {
    const next = new Set(selected);
    next.has(id) ? next.delete(id) : next.add(id);
    selected = next;
  }

  async function generate() {
    // It writes a mod into the mods folder: a library write, one at a time.
    const op = tryStart("Generating a bridge mod", { exclusive: "library-write" });
    if (!op) {
      error = `${blocked("library-write").reason} Try again when it's done.`;
      return;
    }
    generating = true;
    error = null;
    done = null;
    let filename: string | null = null;
    try {
      filename = await generateBridge({
        techName: name.trim(),
        title: "Silo Filltype Bridge",
        fillTypes: fillTypeList,
        categories: [...selected],
      });
      done = filename;
    } catch (e) {
      error = inlineError(e, "Generating the bridge mod");
    } finally {
      generating = false;
      op.end();
    }
    // After the op ends: the rescan it triggers skips while a library write is running.
    if (filename !== null) onGenerated(filename);
  }
</script>

<div class="br">
  <div class="br-head">
    <div>
      <h3>Fill type compatibility bridge</h3>
      <p class="sub">
        Got a map fill type nothing can haul, store, or sell? Silo builds a small companion
        mod that adds it to the categories your equipment accepts, with no edits to other mods.
      </p>
    </div>
    {#if onClose}
      <button class="x" use:tooltip={"Close"} aria-label="Close" onclick={onClose}><Icon name="close" /></button>
    {/if}
  </div>

  {#if done}
    <div class="ok-note">
      Created <b>{done}</b> in your mods folder. Check it's active, then load your save to
      test. The fill type should now work with the checked equipment.
    </div>
  {/if}
  {#if error}<div class="err">{error}</div>{/if}

  <div class="field">
    <label for="br-ft">Fill type names</label>
    <input
      id="br-ft"
      class="txt"
      placeholder="e.g. SUGARBEET  BEETPULP"
      bind:value={fillTypesRaw}
    />
    <p class="hint">
      Their internal names, separated by spaces or commas. Case doesn't matter. If unsure, it's
      usually the crop/material name in caps (check the mod that adds it).
    </p>
    {#if fillTypeList.length > 0}
      <div class="chips">{#each fillTypeList as f (f)}<span class="chip">{f}</span>{/each}</div>
    {/if}
  </div>

  <div class="field" role="group" aria-labelledby="bridge-add-to">
    <span class="lbl" id="bridge-add-to">Add to</span>
    {#each CATEGORY_GROUPS as g (g.label)}
      <div class="grp">
        <div class="grp-label">{g.label}</div>
        <div class="opts">
          {#each g.cats as c (c.id)}
            <button class="opt" class:on={selected.has(c.id)} aria-pressed={selected.has(c.id)} onclick={() => toggle(c.id)}>
              {#if selected.has(c.id)}<Icon name="check" size={13} />{:else}+{/if} {c.label}
            </button>
          {/each}
        </div>
      </div>
    {/each}
  </div>

  <div class="field">
    <label for="br-name">Mod ID</label>
    <input id="br-name" class="txt mono" bind:value={name} />
    <p class="hint">Must start with FS25_. This is the .zip that lands in your mods folder.</p>
  </div>

  <div class="caveat">
    Adds category membership only. It doesn't change balance or realism. If two mods
    both define the fill type, load order still decides its properties. Test in-game after
    generating. If something feels off, delete the .zip to undo it.
  </div>

  <div class="actions">
    {#if onClose}<button class="btn" onclick={onClose}>Close</button>{/if}
    <button class="btn primary" disabled={!canGenerate} onclick={generate}>
      {generating ? "Generating…" : "Generate bridge mod"}
    </button>
  </div>
</div>

<style>
  .br {
    padding: 16px 18px 22px;
  }
  .br-head {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  .br-head h3 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.35rem;
    color: var(--text);
  }
  .sub {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-sm);
    line-height: 1.5;
    max-width: 56ch;
  }
  .x {
    display: inline-grid;
    place-items: center;
    min-width: 28px;
    min-height: 28px;
    border: none;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 1rem;
    align-self: start;
  }
  .x:hover {
    color: var(--text);
  }
  .ok-note {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--text);
    border: 1px solid color-mix(in srgb, var(--primary) 35%, transparent);
    border-radius: var(--radius-sm);
    padding: 9px 12px;
    margin: 8px 0;
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .err {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    margin: 8px 0;
    font-size: 0.85rem;
  }
  .field {
    margin-top: 14px;
  }
  .field label,
  .lbl {
    display: block;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
    margin-bottom: 6px;
  }
  .txt {
    width: 100%;
    padding: 9px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
  }
  .mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85rem;
  }
  .hint {
    margin: 5px 0 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.45;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 7px;
  }
  .chip {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    color: var(--earth-text);
    padding: 2px 8px;
    border-radius: 999px;
  }
  .grp {
    margin-bottom: 10px;
  }
  .grp-label {
    font-size: var(--fs-xs);
    color: var(--text);
    font-weight: 600;
    margin-bottom: 5px;
  }
  .opts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .opt {
    padding: 5px 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--text-muted);
    font: inherit;
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .opt.on {
    border-color: var(--primary);
    background: color-mix(in srgb, var(--primary) 14%, transparent);
    color: var(--primary-text);
    font-weight: 600;
  }
  .caveat {
    margin-top: 14px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
    border-left: 2px solid var(--border);
    padding-left: 10px;
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    margin-top: 16px;
  }
  .btn {
    padding: 8px 16px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .btn.primary {
    background: var(--primary);
    color: var(--on-primary);
    border-color: transparent;
    font-weight: 600;
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
