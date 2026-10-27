<script lang="ts">
  // The Library's "Mod sets…" dialog (#323): add the chosen mods to, or remove them from, several
  // saved sets at once. One box per set you saved (automatic "Before …" snapshots aren't listed):
  // ticked when every chosen mod is in it, unticked when none is, mixed when some are. A click
  // cycles ticked ↔ unticked, and back to mixed for a set that opened mixed. Editing the set
  // you're on also turns the mods on or parks them, and says so beside it.
  //   <SetMembershipDialog bind:open mods={selectedMods} sets={setStore.sets} currentId={…}
  //     {active} onsave={(edits) => setStore.applyMembership(techNames, edits)} />
  import Dialog from "../../ui/Dialog.svelte";
  import Button from "../../ui/Button.svelte";
  import Badge from "../../ui/Badge.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import { formatCount, plural } from "../../plural";
  import type { Loadout } from "../../types";
  import {
    currentSetNote,
    membershipEdits,
    membershipRows,
    next,
    type Membership,
    type MembershipEdit,
    type MembershipRow,
  } from "../../modsets/membership";

  interface Props {
    open?: boolean;
    /** The chosen mods (the selection, or one row's mod). */
    mods: { techName: string; title?: string | null }[];
    /** Every saved set. */
    sets: Loadout[];
    /** The set you're on now (ActiveSetState's id), or null. */
    currentId: number | null;
    /** Tech names active now. */
    active: ReadonlySet<string>;
    /** Why sets can't be edited now (a library change runs), or "". */
    lockReason?: string;
    /** Save the edits; resolves false when nothing was saved (the dialog stays open). */
    onsave: (edits: MembershipEdit[]) => Promise<boolean>;
  }

  let { open = $bindable(false), mods, sets, currentId, active, lockReason = "", onsave }: Props = $props();

  const uid = $props.id();
  const techNames = $derived(mods.map((m) => m.techName));
  const rows = $derived(membershipRows(sets, techNames));
  /** Each set's state now, by id (missing: as it opened). */
  let chosen = $state(new Map<number, Membership>());
  let saving = $state(false);

  const stateOf = (r: MembershipRow) => chosen.get(r.set.id) ?? r.initial;
  const edits = $derived(membershipEdits(rows, techNames, chosen));
  const allUnticked = $derived(rows.every((r) => stateOf(r) === "none"));

  const one = $derived(mods.length === 1 ? (mods[0].title ?? mods[0].techName) : null);
  const title = $derived(one ? `Mod sets for ${one}` : `Mod sets for ${formatCount(mods.length)} mods`);
  const description = $derived(
    one
      ? `Tick a set to add ${one} to it. Untick it to take it out.`
      : "Tick a set to add these mods to it. Untick it to take them out. A mixed box means some of them are in it.",
  );

  function cycle(r: MembershipRow) {
    chosen = new Map(chosen).set(r.set.id, next(stateOf(r), r.initial));
  }

  function untickAll() {
    chosen = new Map(rows.map((r) => [r.set.id, "none"]));
  }

  async function save() {
    if (saving || edits.length === 0) return;
    saving = true;
    try {
      if (await onsave(edits)) open = false;
    } finally {
      saving = false;
    }
  }

  /** Keep a native checkbox's ticked and mixed looks on the row's state (a click toggles it on
   *  its own; this puts it right, mixed included). */
  function tri(node: HTMLInputElement, st: Membership) {
    const apply = (s: Membership) => {
      node.checked = s === "all";
      node.indeterminate = s === "some";
    };
    apply(st);
    return { update: apply };
  }
</script>

<Dialog bind:open {title} {description} size="sm">
  {#if rows.length === 0}
    <EmptyState
      icon="modSets"
      title="No mod sets yet"
      body="Save your mods as a set on the Mod sets page, then add mods to it from here."
      level={3}
    />
  {:else}
    <fieldset class="sets">
      <legend class="sr-only">Your mod sets</legend>
      <ul>
        {#each rows as r (r.set.id)}
          {@const st = stateOf(r)}
          {@const isCurrent = r.set.id === currentId}
          {@const note = isCurrent ? currentSetNote(st, r.initial, techNames, active) : ""}
          <li class:changed={st !== r.initial}>
            <label class="row">
              <input
                type="checkbox"
                use:tri={st}
                aria-describedby={note ? `${uid}-note-${r.set.id}` : undefined}
                onchange={() => cycle(r)}
              />
              <span class="name">{r.set.name}</span>
              {#if isCurrent}<Badge tone="success" icon="ok">Active</Badge>{/if}
              <span class="count tnum"
                >{formatCount(r.set.mods.length)} {plural(r.set.mods.length, { one: "mod", other: "mods" })}</span
              >
            </label>
            {#if note}<span class="note" id="{uid}-note-{r.set.id}">{note}</span>{/if}
          </li>
        {/each}
      </ul>
    </fieldset>
  {/if}

  {#snippet footer()}
    {#if rows.length > 0}
      <Button variant="ghost" class="membership-untick" disabled={allUnticked || saving} onclick={untickAll}>Untick all</Button>
    {/if}
    <Button onclick={() => (open = false)}>Cancel</Button>
    <Button
      variant="primary"
      disabled={edits.length === 0 || saving || !!lockReason}
      title={lockReason || undefined}
      onclick={save}>Save</Button
    >
  {/snippet}
</Dialog>

<style>
  .sets {
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  ul {
    display: grid;
    gap: 2px;
    max-height: 22rem;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
  }
  li {
    display: grid;
    padding: var(--sp-1) var(--sp-2);
    border-radius: var(--r-sm);
  }
  li.changed {
    background: color-mix(in srgb, var(--primary) 8%, transparent);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 1.75rem;
    cursor: pointer;
  }
  input {
    flex: none;
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--primary);
    cursor: inherit;
  }
  .name {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .count {
    margin-left: auto;
    color: var(--muted);
    font-size: var(--fs-xs);
    white-space: nowrap;
  }
  .note {
    padding-left: calc(1rem + var(--sp-2));
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  /* Untick all sits on the left, away from Save. */
  :global(.membership-untick) {
    margin-right: auto;
  }
</style>
