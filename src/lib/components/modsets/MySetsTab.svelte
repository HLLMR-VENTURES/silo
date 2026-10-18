<script lang="ts">
  // Mod sets → My sets: every saved set compared with the active mods, plus the automatic
  // "Before …" snapshots Silo keeps (the newest 5). App owns the actions (createModSets).
  import Button from "../../ui/Button.svelte";
  import Badge from "../../ui/Badge.svelte";
  import MenuButton from "../../ui/MenuButton.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import type { MenuItem } from "../../ui/overlayTypes";
  import type { Loadout } from "../../types";
  import { formatCount, plural } from "../../plural";
  import { compareSet, shortDate, sortSets, type ActiveSetState } from "../../modsets/modSets";
  import type { Snippet } from "svelte";

  interface Props {
    sets: Loadout[];
    active: ReadonlySet<string>;
    /** Tech names in the library, or undefined before the first scan. */
    library: ReadonlySet<string> | undefined;
    state: ActiveSetState;
    /** Why sets can't be saved, replaced or deleted now (a foreground library change), or "". */
    lockReason?: string;
    onSwitch: (set: Loadout) => void;
    onUpdate: (set: Loadout) => void;
    onDelete: (set: Loadout) => void;
    onShare: (name: string, mods: string[]) => void;
    onSaveCurrent: () => void;
    onOpenShared: () => void;
    /** Below the table (your shared links). */
    after?: Snippet;
  }

  let {
    sets,
    active,
    library,
    state,
    lockReason = "",
    onSwitch,
    onUpdate,
    onDelete,
    onShare,
    onSaveCurrent,
    onOpenShared,
    after,
  }: Props = $props();

  const rows = $derived(
    sortSets(sets).map((set) => {
      const cmp = compareSet(set.mods, active, library);
      return { set, cmp, missing: cmp.diff.missing.length };
    }),
  );
  const currentSet = $derived(state.kind === "custom" ? sets.find((s) => s.id === state.id) : undefined);
  /** Only a set you saved can take your edits; an automatic one would be pruned later. */
  const canUpdateCurrent = $derived(currentSet?.kind === "user");

  function menuFor(set: Loadout): MenuItem[] {
    const lock = !!lockReason;
    if (set.kind === "auto")
      return [
        { id: "share", label: "Share…", icon: "share" },
        { id: "delete", label: "Delete", icon: "close", danger: true, disabled: lock, separatorBefore: true },
      ];
    return [
      { id: "update", label: "Update from current mods", icon: "rescan", disabled: lock || active.size === 0 },
      { id: "share", label: "Share…", icon: "share" },
      { id: "delete", label: "Delete", icon: "close", danger: true, disabled: lock, separatorBefore: true },
    ];
  }
  function onMenu(set: Loadout, id: string) {
    if (id === "update") onUpdate(set);
    else if (id === "share") onShare(set.name, set.mods);
    else if (id === "delete") onDelete(set);
  }
</script>

<div class="my-sets">
  <div class="toolbar">
    <Button
      variant="primary"
      icon="modSets"
      disabled={active.size === 0 || !!lockReason}
      title={lockReason || (active.size === 0 ? "Turn on some mods first" : undefined)}
      onclick={onSaveCurrent}>Save current mods as a set</Button
    >
    <Button icon="share" disabled={active.size === 0} onclick={() => onShare("My mods", [...active])}
      >Share my active mods</Button
    >
    <Button icon="link" onclick={onOpenShared}>Open a shared set</Button>
  </div>

  <!-- Always in the page, so the change is announced when the banner's text appears. -->
  <div role="status" class="custom-region">
    {#if state.kind === "custom" && currentSet}
      <div class="custom">
        <span
          >Your active mods have {formatCount(state.changes)}
          {plural(state.changes, { one: "change", other: "changes" })} since <strong>{state.name}</strong>.</span
        >
        {#if canUpdateCurrent}
          <Button size="sm" disabled={!!lockReason} title={lockReason || undefined} onclick={() => onUpdate(currentSet)}
            >Update the set</Button
          >
        {/if}
        <Button
          size="sm"
          variant={canUpdateCurrent ? "ghost" : "secondary"}
          disabled={!!lockReason}
          title={lockReason || undefined}
          onclick={onSaveCurrent}>Save as new</Button
        >
      </div>
    {/if}
  </div>

  {#if rows.length === 0}
    <EmptyState
      icon="modSets"
      title="No mod sets yet"
      body="Turn on the mods you want, then save them as a set. Switching between sets later shows what changes and can be undone."
      level={3}
    />
  {:else}
    <table class="sets">
      <caption class="sr-only">Your mod sets</caption>
      <thead>
        <tr>
          <th scope="col">Name</th>
          <th scope="col" class="num">Mods</th>
          <th scope="col">Compared with your active mods</th>
          <th scope="col"><span class="sr-only">Actions</span></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as { set, cmp, missing } (set.id)}
          {@const isActive = state.kind === "set" && state.id === set.id}
          <tr class:active={isActive} class:auto={set.kind === "auto"}>
            <th scope="row" class="name">
              <span class="title">{set.name}</span>
              {#if isActive}<Badge tone="success">Active</Badge>{/if}
              {#if set.kind === "auto"}
                <span class="meta">Automatic{set.createdAt ? ` · ${shortDate(set.createdAt)}` : ""}</span>
              {/if}
            </th>
            <td class="num tnum">{formatCount(set.mods.length)}</td>
            <td>
              <span class="cmp tnum" class:same={cmp.same} aria-hidden="true">{cmp.text}</span>
              <span class="sr-only">{cmp.label}</span>
              {#if missing > 0}
                <span class="meta">{formatCount(missing)} not in your library</span>
              {/if}
            </td>
            <td class="actions">
              {#if !cmp.same}
                <Button size="sm" onclick={() => onSwitch(set)}
                  >{set.kind === "auto" ? "Switch back" : "Switch to this set"}</Button
                >
              {/if}
              <MenuButton
                label={`More for ${set.name}`}
                icon="more"
                iconOnly
                variant="ghost"
                size="sm"
                items={menuFor(set)}
                onselect={(id) => onMenu(set, id)}
              />
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="hint">
      Before each switch Silo saves your mods as an automatic set, so you can switch back. It keeps the newest 5.
    </p>
  {/if}

  {@render after?.()}
</div>

<style>
  .my-sets {
    max-width: 64rem;
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .custom {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2) var(--sp-3);
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--border);
    border-left: 3px solid var(--accent, var(--primary));
    border-radius: var(--radius);
    background: var(--surface-raised);
    font-size: var(--fs-sm);
  }
  .custom-region:empty {
    display: none;
  }
  .custom span {
    flex: 1 1 20rem;
  }
  .sets {
    width: 100%;
    border-collapse: collapse;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    font-size: var(--fs-sm);
  }
  th,
  td {
    text-align: left;
    padding: var(--sp-2) var(--sp-3);
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  thead th {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  tbody tr:last-child th,
  tbody tr:last-child td {
    border-bottom: none;
  }
  tr.active {
    background: color-mix(in srgb, var(--primary) 8%, transparent);
  }
  .name {
    font-weight: 600;
  }
  .name .title {
    margin-right: var(--sp-2);
  }
  tr.auto .name {
    font-weight: 500;
  }
  .meta {
    display: block;
    font-weight: 400;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-top: 2px;
  }
  .num {
    text-align: right;
    width: 5rem;
  }
  .cmp.same {
    color: var(--text-muted);
  }
  .actions {
    text-align: right;
    white-space: nowrap;
    width: 1%;
  }
  .actions :global(> *) {
    vertical-align: middle;
  }
  .hint {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
