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
  import { displayVersion } from "../../format";
  import {
    droppedBuildFor,
    normVersion,
    sourceLabel,
    versionChange,
    type UpdateSections,
  } from "../../updates/sections";
  import type { ExplainedError } from "../../updates/run";
  import { UPDATE_PRIVACY_LINE } from "../../updates/autoCheck.svelte";

  /** One row's state: updating, failed with the reason, or (Couldn't check) checking again. */
  export type RowInstallState =
    | { phase: "installing" }
    | { phase: "checking" }
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
    /** Why installs are off (a library write or a check is running), or "". */
    lockReason: string;
    /** Linked GitHub repos were checked even if the catalog didn't answer. */
    hasLinkedRepos?: boolean;
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
    /** Switch update checks back on for a mod under Not checked (#333). */
    onCheckAgain?: (techName: string) => void;
  }

  let {
    sections,
    checked,
    checking,
    catalogError,
    rowStates,
    updatingAll,
    lockReason,
    hasLinkedRepos = false,
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
    onCheckAgain,
  }: Props = $props();

  const s = $derived(sections);
  const needsAction = $derived(
    s.ready.length + s.modhub.length + s.checkYourself.length + s.failed.length,
  );
  const n = (count: number) => formatCount(count);
  const isModHubPage = (url: string | null | undefined) => !!url && /farming-simulator\.com/i.test(url);

  // Up to date and Not tracked start collapsed (this view visit).
  let openUpToDate = $state(false);
  let openNotTracked = $state(false);
  let openSkipped = $state(false);
</script>

{#snippet sectionHead(id: string, title: string, count: number)}
  <h3 class="sec-title" {id}>{title} <span class="count tnum">({n(count)})</span></h3>
{/snippet}

{#snippet foldHead(id: string, title: string, count: number, open: boolean, toggle: () => void)}
  <h3 class="sec-title">
    <button type="button" class="fold-btn" aria-expanded={open} aria-controls={`${id}-body`} onclick={toggle}>
      <Icon name={open ? "chevronDown" : "chevronRight"} size={16} />
      {title} <span class="count tnum">({n(count)})</span>
    </button>
  </h3>
{/snippet}

{#snippet copyButton(detail: string, name: string)}
  <Button size="sm" variant="ghost" icon="copy" aria-label={`Copy details: ${name}`} onclick={() => onCopyDetails(detail)}>
    Copy details
  </Button>
{/snippet}

<div class="updates">
  <!-- A first check whose catalog request failed shows the failure, not "never checked". -->
  {#if !checked && !checking && !catalogError}
    <EmptyState icon="update" title="Check your mods for updates" body={UPDATE_PRIVACY_LINE} level={3}>
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
        level={3}
        icon="cloud"
        title={catalogError.offline
          ? "Silo can't reach its catalog right now. Your library still works."
          : "Silo's catalog couldn't answer this check. Your library still works."}
        body={hasLinkedRepos
          ? "Linked GitHub mods were still checked. Try again in a moment."
          : catalogError.offline
            ? "Try again in a moment."
            : "Try again in a moment. If it keeps happening, copy the details into a bug report."}
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
        <!-- Always mounted, so screen readers announce the reason when it appears. -->
        <p class="reason" role="status">{lockReason}</p>
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
                  <p class="row-error" role="status"><Icon name="problem" size={14} /> {st.message}</p>
                {/if}
              </div>
              <div class="acts">
                {#if st?.phase === "installing"}
                  <span class="busy" role="status">Updating…</span>
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
                    <Icon name="check" size={14} /> Found {displayVersion(dropped.flatVersion)} in your mods folder.
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
                  <Button size="sm" aria-label={`${isModHubPage(r.pageUrl) ? "Open on ModHub" : "Open page"}: ${r.title}`} onclick={() => onOpenPage(r)}>
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
                <Button size="sm" aria-label={`Open page: ${c.title}`} onclick={() => onOpenSource(c)}>
                  Open page <Icon name="external" size={12} />
                </Button>
                <Button size="sm" variant="ghost" aria-label={`I checked it: ${c.title}`} onclick={() => onMarkChecked(c)}>
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
            {@const rechecking = rowStates[r.techName]?.phase === "checking"}
            <li class="row">
              <div class="main">
                <div class="name">{r.title}</div>
                <p class="row-error"><Icon name="problem" size={14} /> {r.error}</p>
              </div>
              <div class="acts">
                <span use:tooltip={checking ? "Wait for the check to finish." : null}>
                  <Button size="sm" icon="rescan" loading={rechecking} disabled={checking || rechecking} aria-label={`Try again: check ${r.title}`} onclick={() => onRecheck(r)}>
                    Try again
                  </Button>
                </span>
                {#if r.errorDetail}{@render copyButton(r.errorDetail, r.title)}{/if}
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if s.upToDate.length > 0}
      <section class="sec fold">
        {@render foldHead("up-current", "Up to date", s.upToDate.length, openUpToDate, () => (openUpToDate = !openUpToDate))}
        {#if openUpToDate}
          <ul class="rows compact" id="up-current-body">
            {#each s.upToDate as r (r.techName)}
              {@const v = r.current ?? r.latest}
              <li class="row">
                <span class="name">{r.title}</span>
                <span class="detail tnum">{v ? normVersion(v) : ""}</span>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if s.notTracked.length > 0}
      <section class="sec fold">
        {@render foldHead("up-untracked", "Not tracked", s.notTracked.length, openNotTracked, () => (openNotTracked = !openNotTracked))}
        <p class="help">
          Silo's catalog doesn't know {plural(s.notTracked.length, { one: "this mod", other: "these mods" })}.
          If you got one from a website, save that page in the mod's details and Silo lists it under
          Check these yourself.
        </p>
        {#if openNotTracked}
          <ul class="rows compact" id="up-untracked-body">
            {#each s.notTracked as m (m.techName)}
              <li class="row"><span class="name">{m.title}</span></li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if s.skipped.length > 0}
      <section class="sec fold">
        {@render foldHead("up-skipped", "Not checked", s.skipped.length, openSkipped, () => (openSkipped = !openSkipped))}
        <p class="help">
          You switched update checks off for {plural(s.skipped.length, { one: "this mod", other: "these mods" })},
          so Silo leaves {plural(s.skipped.length, { one: "it", other: "them" })} out of checks and Update all.
        </p>
        {#if openSkipped}
          <ul class="rows compact" id="up-skipped-body">
            {#each s.skipped as m (m.techName)}
              <li class="row">
                <span class="name">{m.title}</span>
                {#if onCheckAgain}
                  <div class="acts">
                    <Button size="sm" variant="ghost" aria-label={`Check for updates: ${m.title}`} onclick={() => onCheckAgain(m.techName)}>
                      Check for updates
                    </Button>
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        {/if}
      </section>
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
  .fold-btn {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    cursor: pointer;
  }
  .reason:empty {
    margin: 0;
  }
</style>
