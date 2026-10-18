<script lang="ts">
  // Mod sets → Savegames: one card per save with its map, when it was last played and how many of
  // its mods are in the library. "Use this save's mods" goes through the same switch confirmation
  // as a set (and saves a set named after the save); missing mods can be looked up in Browse.
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import Icon from "../../ui/Icon.svelte";
  import type { Savegame } from "../../types";
  import { saveDateLabel, saveModsLine, type SaveModStats } from "../../modsets/modSets";
  import { formatCount } from "../../plural";

  interface Props {
    rows: { s: Savegame; st: SaveModStats }[];
    /** Why Back up is off (a library write is running), or "". */
    lockReason?: string;
    onUse: (s: Savegame) => void;
    onBackup: (s: Savegame) => void;
    onSettings: (s: Savegame) => void;
    onFindInBrowse: (techName: string, title: string | null) => void;
  }

  let { rows, lockReason = "", onUse, onBackup, onSettings, onFindInBrowse }: Props = $props();

  /** Folders whose missing-mod list is open. */
  let shown = $state<Record<string, boolean>>({});
</script>

{#if rows.length === 0}
  <EmptyState
    icon="map"
    title="No savegames yet"
    body="Savegames appear here once you've saved a game in Farming Simulator 25."
    level={3}
  />
{:else}
  <ul class="saves">
    {#each rows as { s, st } (s.folder)}
      {@const played = saveDateLabel(s.saveDate)}
      {@const listId = `missing-${s.folder}`}
      <li class="card">
        <div class="head">
          <h3>{s.name}</h3>
          <span class="slot">Slot {s.index}</span>
        </div>
        <p class="meta">
          {#if s.mapTitle}<span><Icon name="map" size={14} /> {s.mapTitle}</span>{/if}
          {#if played}<span>Last played {played}</span>{/if}
        </p>
        <p class="mods" class:warn={st.missing.length > 0}>
          {saveModsLine(st)}{#if st.missing.length > 0}. {formatCount(st.missing.length)} missing{/if}
        </p>
        <div class="actions">
          <Button
            variant="primary"
            size="sm"
            disabled={st.present.length === 0}
            title={st.present.length === 0 ? "None of this save's mods are in your library yet" : undefined}
            onclick={() => onUse(s)}>Use this save's mods</Button
          >
          <Button size="sm" disabled={!!lockReason} title={lockReason || "A safe copy; the save itself isn't changed"} onclick={() => onBackup(s)}
            >Back up</Button
          >
          <Button size="sm" variant="ghost" icon="modSettings" onclick={() => onSettings(s)}>Game settings…</Button>
          {#if st.missing.length > 0}
            <Button
              size="sm"
              variant="ghost"
              icon={shown[s.folder] ? "chevronUp" : "chevronDown"}
              aria-expanded={!!shown[s.folder]}
              aria-controls={listId}
              onclick={() => (shown[s.folder] = !shown[s.folder])}
              >{shown[s.folder] ? "Hide missing mods" : "Show missing mods"}</Button
            >
          {/if}
        </div>
        {#if st.missing.length > 0 && shown[s.folder]}
          <ul class="missing" id={listId}>
            {#each st.missing as m (m.modName)}
              <li>
                <span class="mn">{m.title ?? m.modName}</span>
                <Button size="sm" variant="ghost" onclick={() => onFindInBrowse(m.modName, m.title)}>Find in Browse</Button>
              </li>
            {/each}
          </ul>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .saves {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(22rem, 1fr));
    gap: var(--sp-3);
    max-width: 72rem;
  }
  .card {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--sp-2);
  }
  h3 {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .slot,
  .meta {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .meta {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1) var(--sp-3);
  }
  .meta span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .mods {
    margin: 0;
    font-size: var(--fs-sm);
  }
  .mods.warn {
    color: var(--warn-text);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
    margin-top: var(--sp-1);
  }
  .missing {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border);
  }
  .missing li {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-sm);
  }
  .mn {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
