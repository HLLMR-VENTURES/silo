<script lang="ts">
  // The links you've shared (unlisted pages on your GitHub): copy one again, point it at your
  // current mods, or stop sharing it. Shown only once GitHub can hold links.
  import Button from "../../ui/Button.svelte";
  import { ask, toast } from "../../ui";
  import {
    collectionDelete,
    collectionsList,
    collectionUpdate,
    copyText,
    ghStatus,
    openExternal,
    type CollectionSummary,
  } from "../../api";
  import { inlineError } from "../../state/notify.svelte";
  import type { MpModRef } from "../../types";
  import { formatCount, plural } from "../../plural";

  interface Props {
    /** The active mods as refs: what "Update link" pins the link to. */
    active: MpModRef[];
    /** Bump to list again (after a new link was created). */
    refreshKey?: number;
  }
  let { active, refreshKey = 0 }: Props = $props();

  let canList = $state(false);
  let links = $state<CollectionSummary[]>([]);
  let loading = $state(false);
  let error = $state("");
  /** Only part of the list could be read (gists or repos failed). */
  let warning = $state("");
  let busyRef = $state<string | null>(null);

  const zips = $derived(active.filter((m) => m.kind === "zip").length);
  const count = (n: number) => `${formatCount(n)} ${plural(n, { one: "mod", other: "mods" })}`;

  async function load() {
    try {
      const s = await ghStatus();
      canList = s.canGist || s.canWrite;
    } catch {
      canList = false;
    }
    if (!canList) return;
    loading = true;
    error = "";
    try {
      const list = await collectionsList();
      links = list.items;
      warning = list.warning ?? "";
    } catch (e) {
      warning = "";
      error = inlineError(e, "Loading your shared links");
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    void refreshKey;
    void load();
  });

  function sharedOn(c: CollectionSummary): string {
    if (!c.createdAt) return "Shared";
    const d = new Date(c.createdAt);
    if (Number.isNaN(d.getTime())) return "Shared";
    return `Shared ${new Intl.DateTimeFormat("en", { month: "short", day: "numeric" }).format(d)}`;
  }

  async function copy(c: CollectionSummary) {
    try {
      await copyText(c.pageUrl);
      toast({ message: "Link copied." });
    } catch {
      error = "Silo couldn't use the clipboard.";
    }
  }

  async function update(c: CollectionSummary) {
    if (busyRef) return;
    if (
      !(await ask({
        title: `Update the link for ${c.name}?`,
        body: `It will list your current ${count(zips)} instead of ${count(c.modCount)}. The link stays the same, so friends who have it see the new list.`,
        confirm: "Update link",
      }))
    )
      return;
    busyRef = c.reference;
    error = "";
    try {
      const r = await collectionUpdate(c.reference, active);
      const unpinned = r.unpinned.length
        ? ` Silo couldn't read ${count(r.unpinned.length)}, so the link lists ${r.unpinned.length === 1 ? "it" : "them"} without a fingerprint.`
        : "";
      toast({ message: `Updated the link for ${c.name} to ${count(r.count)}.${unpinned}` });
      await load();
    } catch (e) {
      error = inlineError(e, "Updating the link");
    } finally {
      busyRef = null;
    }
  }

  async function stop(c: CollectionSummary) {
    if (busyRef) return;
    if (!c.canDelete) {
      // Public repos are removed on GitHub itself.
      void openExternal(c.sourceUrl);
      return;
    }
    if (
      !(await ask({
        title: `Stop sharing ${c.name}?`,
        body: "Silo removes it from your GitHub and the link stops working. This can't be undone, but you can share the set again with a new link.",
        confirm: "Stop sharing",
        tone: "danger",
      }))
    )
      return;
    busyRef = c.reference;
    error = "";
    try {
      await collectionDelete(c.kind, c.reference);
      links = links.filter((x) => x.reference !== c.reference);
    } catch (e) {
      error = inlineError(e, "Stopping sharing");
    } finally {
      busyRef = null;
    }
  }
</script>

{#if canList}
  <section class="links" aria-labelledby="shared-links-h">
    <h3 id="shared-links-h">Your shared links</h3>
    {#if loading && links.length === 0}
      <p class="muted">Loading your shared links…</p>
    {:else if error}
      <p class="err" role="status">{error}</p>
    {:else if links.length === 0 && !warning}
      <p class="muted">Nothing shared yet. Use Share on a set to create a link.</p>
    {/if}
    {#if !error && warning}<p class="warn" role="status">{warning}</p>{/if}
    {#each links as c (c.reference)}
      <div class="row">
        <div class="info">
          <span class="name">{c.name}</span>
          <span class="muted">{sharedOn(c)} · {count(c.modCount)}</span>
        </div>
        <Button size="sm" icon="copy" aria-label="Copy link: {c.name}" onclick={() => copy(c)}>Copy link</Button>
        <Button size="sm" aria-label="Update link: {c.name}" disabled={busyRef === c.reference || zips === 0} onclick={() => update(c)}>Update link</Button>
        <Button
          size="sm"
          variant="ghost"
          aria-label="{c.canDelete ? 'Stop sharing' : 'Stop sharing on GitHub'}: {c.name}"
          disabled={busyRef === c.reference}
          onclick={() => stop(c)}
          >{c.canDelete ? "Stop sharing" : "Stop sharing on GitHub"}</Button
        >
      </div>
    {/each}
  </section>
{/if}

<style>
  .links {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  h3 {
    margin: var(--sp-2) 0 0;
    font-family: var(--font-display);
    font-size: var(--fs-md);
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .info {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-weight: 600;
    font-size: var(--fs-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .muted {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .warn {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--warn-text);
  }
  .err {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--danger);
  }
</style>
