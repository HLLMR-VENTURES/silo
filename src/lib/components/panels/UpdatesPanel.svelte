<script lang="ts">
  // The Updates view's body: the sections of one check, in order, empty ones hidden.
  //   Ready to install (with Update all) · Download from ModHub · Check these yourself ·
  //   Couldn't check · Up to date (collapsed) · Not tracked (collapsed, with help)
  // Before any check it's the "Check your mods for updates" empty state; a catalog that didn't
  // answer shows in place with Try again. Nothing installs unless a button here is pressed.
  //   <UpdatesPanel {sections} checked={true} ... onUpdateAll={...} />
  import Badge from "../../ui/Badge.svelte";
  import Button from "../../ui/Button.svelte";
  import EmptyState from "../../ui/EmptyState.svelte";
  import ErrorState from "../../ui/ErrorState.svelte";
  import Icon from "../../ui/Icon.svelte";
  import { tooltip } from "../../ui/tooltip";
  import type { ForeignFile } from "../../api";
  import type { UpdateRow } from "../../types";
  import { checkedDate, type CheckYourselfRow } from "../../sources";
  import { formatCount, plural } from "../../plural";
  import {
    droppedBuildFor,
    sourceLabel,
    versionChange,
    type UpdateSections,
  } from "../../updates/sections";
  import type { ExplainedError } from "../../updates/run";
  import { UPDATE_PRIVACY_LINE } from "../../updates/autoCheck.svelte";

  /** One row's install: running, or failed with the reason. */
  export type RowInstallState =
    | { phase: "installing" }
    | { phase: "error"; message: string; detail: string };

  interface Props {
    sections: UpdateSections;
    /** A check has finished this session. */
    checked: boolean;
    checking: boolean;
    /** The catalog request failed in the last check. */
    catalogError: ExplainedError | null;
    rowStates: Record<string, RowInstallState>;
    /** Update all is running. */
    updatingAll: boolean;
    /** Why installs are off (a library write is running), or "". */
    lockReason: string;
    /** Why Check now is off, or "". */
    checkDisabledReason: string;
    /** Files in the mods folder at a mod's name that Silo didn't put there (saved from ModHub). */
    foreign: ForeignFile[];
    onCheck: () => void;
    onInstall: (row: UpdateRow) => void;
    onUpdateAll: () => void;
    onOpenPage: (row: UpdateRow) => void;
    onUseDropped: (file: ForeignFile, row: UpdateRow) => void;
    onOpenSource: (row: CheckYourselfRow) => void;
    onMarkChecked: (row: CheckYourselfRow) => void;
    onRecheck: (row: UpdateRow) => void;
    onCopyDetails: (text: string) => void;
  }

  let {
    sections,
    checked,
    checking,
    catalogError,
    rowStates,
    updatingAll,
    lockReason,
    checkDisabledReason,
    foreign,
    onCheck,
    onInstall,
    onUpdateAll,
    onOpenPage,
    onUseDropped,
    onOpenSource,
    onMarkChecked,
    onRecheck,
    onCopyDetails,
  }: Props = $props();

  const s = $derived(sections);
  const needsAction = $derived(
    s.ready.length + s.modhub.length + s.checkYourself.length + s.failed.length,
  );
  const n = (count: number) => formatCount(count);
  const isModHubPage = (url: string | null | undefined) => !!url && /farming-simulator\.com/i.test(url);
</script>

{#snippet sectionHead(id: string, title: string, count: number)}
  <h2 class="sec-title" {id}>{title} <span class="count tnum">({n(count)})</span></h2>
{/snippet}

{#snippet copyButton(detail: string, name: string)}
  <Button size="sm" variant="ghost" icon="copy" aria-label={`Copy details: ${name}`} onclick={() => onCopyDetails(detail)}>
    Copy details
  </Button>
{/snippet}

<div class="updates">
  {#if !checked && !checking}
    <EmptyState icon="update" title="Check your mods for updates" body={UPDATE_PRIVACY_LINE}>
      {#snippet actions()}
        <span use:tooltip={checkDisabledReason || null}>
          <Button variant="primary" icon="update" disabled={!!checkDisabledReason} onclick={onCheck}>
            Check now
          </Button>
        </span>
      {/snippet}
    </EmptyState>
  {:else}
    {#if catalogError && !checking}
      <ErrorState
        icon="cloud"
        level={2}
        title="Silo can't reach its catalog right now. Your library still works."
        body="Linked GitHub mods were still checked. Try again in a moment."
        details={catalogError.detail}
      >
        {#snippet actions()}
          <Button icon="rescan" disabled={!!checkDisabledReason} onclick={onCheck}>Try again</Button>
        {/snippet}
      </ErrorState>
    {:else if checked && !checking && needsAction === 0 && s.upToDate.length > 0}
      <p class="all-good"><Icon name="ok" size={16} /> Every mod Silo can check is up to date.</p>
    {/if}

    {#if s.ready.length > 0}
      <section class="sec" aria-labelledby="up-ready">
        <div class="sec-head">
          {@render sectionHead("up-ready", "Ready to install", s.ready.length)}
          <span use:tooltip={lockReason || null}>
            <Button
              variant="primary"
              icon="download"
              loading={updatingAll}
              disabled={!!lockReason || updatingAll}
              onclick={onUpdateAll}
            >
              Update all {n(s.ready.length)}
            </Button>
          </span>
        </div>
        {#if lockReason}<p class="reason" role="status">{lockReason}</p>{/if}
        <ul class="rows">
          {#each s.ready as r (r.techName)}
            {@const st = rowStates[r.techName]}
            {@const src = sourceLabel(r)}
            <li class="row">
              <div class="main">
                <div class="name">
                  {r.title}
                  {#if src}<Badge>{src}</Badge>{/if}
                </div>
                <div class="detail tnum">{versionChange(r)}</div>
                {#if st?.phase === "error"}
                  <p class="row-error" role="alert"><Icon name="problem" size={14} /> {st.message}</p>
                {/if}
              </div>
              <div class="acts">
                {#if st?.phase === "installing"}
                  <span class="busy" role="status">Installing…</span>
                {:else if st?.phase === "error"}
                  <span use:tooltip={lockReason || null}>
                    <Button size="sm" icon="rescan" disabled={!!lockReason} aria-label={`Try again: update ${r.title}`} onclick={() => onInstall(r)}>
                      Try again
                    </Button>
                  </span>
                  {@render copyButton(st.detail, r.title)}
                {:else}
                  <span use:tooltip={lockReason || null}>
                    <Button size="sm" disabled={!!lockReason} aria-label={`Update ${r.title}`} onclick={() => onInstall(r)}>
                      Update
                    </Button>
                  </span>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if s.modhub.length > 0}
      <section class="sec" aria-labelledby="up-modhub">
        <div class="sec-head">{@render sectionHead("up-modhub", "Download from ModHub", s.modhub.length)}</div>
        <p class="help">
          ModHub mods download from GIANTS' website. Save the new zip into your mods folder and Silo
          offers to use it.
        </p>
        <ul class="rows">
          {#each s.modhub as r (r.techName)}
            {@const dropped = droppedBuildFor(r, foreign)}
            <li class="row">
              <div class="main">
                <div class="name">{r.title}</div>
                <div class="detail tnum">{versionChange(r)}</div>
                {#if dropped}
                  <p class="found">
                    <Icon name="check" size={14} /> Found {dropped.flatVersion} in your mods folder.
                  </p>
                {/if}
              </div>
              <div class="acts">
                {#if dropped}
                  <span use:tooltip={lockReason || null}>
                    <Button size="sm" variant="primary" disabled={!!lockReason} aria-label={`Use this version of ${r.title}`} onclick={() => onUseDropped(dropped, r)}>
                      Use this version
                    </Button>
                  </span>
                {/if}
                {#if r.pageUrl}
                  <Button size="sm" aria-label={`Open ${r.title} on ${isModHubPage(r.pageUrl) ? "ModHub" : "its page"}`} onclick={() => onOpenPage(r)}>
                    {isModHubPage(r.pageUrl) ? "Open on ModHub" : "Open page"} <Icon name="external" size={12} />
                  </Button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if s.checkYourself.length > 0}
      <section class="sec" aria-labelledby="up-yourself">
        <div class="sec-head">{@render sectionHead("up-yourself", "Check these yourself", s.checkYourself.length)}</div>
        <p class="help">
          Silo can't check these automatically. Open the page you saved to see if there's a newer
          version.
        </p>
        <ul class="rows">
          {#each s.checkYourself as c (c.techName)}
            <li class="row">
              <div class="main">
                <div class="name">{c.title}</div>
                <div class="detail">
                  {c.host} · {c.lastChecked ? `last checked ${checkedDate(c.lastChecked)}` : "not checked yet"}
                </div>
              </div>
              <div class="acts">
                <Button size="sm" aria-label={`Open the page for ${c.title}`} onclick={() => onOpenSource(c)}>
                  Open page <Icon name="external" size={12} />
                </Button>
                <Button size="sm" variant="ghost" aria-label={`I checked ${c.title}`} onclick={() => onMarkChecked(c)}>
                  I checked it
                </Button>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if s.failed.length > 0}
      <section class="sec" aria-labelledby="up-failed">
        <div class="sec-head">{@render sectionHead("up-failed", "Couldn't check", s.failed.length)}</div>
        <ul class="rows">
          {#each s.failed as r (r.techName)}
            <li class="row">
              <div class="main">
                <div class="name">{r.title}</div>
                <p class="row-error"><Icon name="problem" size={14} /> {r.error}</p>
              </div>
              <div class="acts">
                <Button size="sm" icon="rescan" disabled={checking} aria-label={`Try again: check ${r.title}`} onclick={() => onRecheck(r)}>
                  Try again
                </Button>
                {#if r.errorDetail}{@render copyButton(r.errorDetail, r.title)}{/if}
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if s.upToDate.length > 0}
      <details class="sec fold">
        <summary><h2 class="sec-title">Up to date <span class="count tnum">({n(s.upToDate.length)})</span></h2></summary>
        <ul class="rows compact">
          {#each s.upToDate as r (r.techName)}
            <li class="row">
              <span class="name">{r.title}</span>
              <span class="detail tnum">{r.current ?? r.latest ?? ""}</span>
            </li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if s.notTracked.length > 0}
      <details class="sec fold">
        <summary><h2 class="sec-title">Not tracked <span class="count tnum">({n(s.notTracked.length)})</span></h2></summary>
        <p class="help">
          Silo's catalog doesn't know {plural(s.notTracked.length, { one: "this mod", other: "these mods" })}.
          If you got one from a website, save that page in the mod's details and Silo lists it under
          Check these yourself.
        </p>
        <ul class="rows compact">
          {#each s.notTracked as m (m.techName)}
            <li class="row"><span class="name">{m.title}</span></li>
          {/each}
        </ul>
      </details>
    {/if}
  {/if}
</div>

<style>
  .updates {
    max-width: 52rem;
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
  }
  .sec {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--sp-3) var(--sp-4);
  }
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    flex-wrap: wrap;
  }
  .sec-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-md);
    font-weight: 600;
    display: inline;
  }
  .count {
    color: var(--text-muted);
    font-family: var(--font-ui);
    font-weight: 500;
  }
  .help {
    margin: var(--sp-2) 0 0;
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .rows {
    list-style: none;
    margin: var(--sp-2) 0 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-3);
    padding: var(--sp-2) 0;
    border-top: 1px solid var(--border);
  }
  .rows .row:first-child {
    border-top: 0;
  }
  .compact .row {
    padding: var(--sp-1) 0;
  }
  .main {
    min-width: 0;
  }
  .name {
    font-weight: 600;
    font-size: var(--fs-sm);
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    flex-wrap: wrap;
  }
  .detail {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-top: 2px;
  }
  .tnum {
    font-variant-numeric: tabular-nums;
  }
  .acts {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex: 0 0 auto;
  }
  .reason {
    margin: var(--sp-1) 0 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .busy {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .row-error {
    margin: var(--sp-1) 0 0;
    font-size: var(--fs-xs);
    color: var(--danger);
    display: flex;
    gap: var(--sp-1);
    align-items: flex-start;
  }
  .found {
    margin: var(--sp-1) 0 0;
    font-size: var(--fs-xs);
    color: var(--success);
    display: flex;
    gap: var(--sp-1);
    align-items: center;
  }
  .all-good {
    margin: 0;
    display: flex;
    gap: var(--sp-2);
    align-items: center;
    font-size: var(--fs-sm);
    color: var(--success);
  }
  .fold summary {
    cursor: pointer;
    list-style-position: inside;
  }
  .fold summary::marker {
    color: var(--text-muted);
  }
</style>
