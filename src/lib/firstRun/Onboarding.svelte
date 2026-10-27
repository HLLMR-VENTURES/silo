<script lang="ts">
  // First run (#250): a full-window, three-step setup over the app, shown only for a new
  // library (firstRun.ts decides). Nothing moves without a click: the folder choices go through
  // the app's own pickers and switches, and organizing only starts from "Organize N mods".
  //   <Onboarding {modsStatus} gameFound={!!gameInfo} ... onfinish={(how) => ...} />
  // "Skip setup" (top right) and "Not now" land in the Library with nothing moved.
  import { tick } from "svelte";
  import Button from "../ui/Button.svelte";
  import Checkbox from "../ui/Checkbox.svelte";
  import Icon from "../ui/Icon.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import RadioGroup from "../ui/RadioGroup.svelte";
  import SiloMark from "../ui/SiloMark.svelte";
  import { portal } from "../ui/portal";
  import { pushLayer } from "../ui/overlayStack";
  import OrganizeSummary from "../components/organize/OrganizeSummary.svelte";
  import OrganizeFiles from "../components/organize/OrganizeFiles.svelte";
  import type { ModsRootStatus } from "../api";
  import type { PlannedMove } from "../types";
  import { formatCount } from "../plural";
  import {
    failureDetails,
    filingProgress,
    folderUsable,
    folderVariant,
    nMods,
    type ArchiveChoice,
    type FilingFailure,
    type FinishHow,
    type OrganizeOutcome,
  } from "./firstRun";

  interface Props {
    modsStatus: ModsRootStatus | null;
    /** Farming Simulator 25 was found (Play appears). */
    gameFound: boolean;
    /** The mods folder is cloud-synced: active mods are copied, not linked. */
    cloudSynced?: boolean;
    scanning: boolean;
    /** The first scan of this folder finished. */
    scanned: boolean;
    /** The last scan failed (its notice says why). */
    scanFailed?: boolean;
    /** Scan the folder again. */
    onRescan?: () => void | Promise<void>;
    /** Auto-file is on already (the step 2 box starts ticked). */
    autoFileOn?: boolean;
    progress: { done: number; total: number };
    modCount: number;
    activeCount: number;
    /** Unzipped folder mods (organizing leaves them where they are). */
    folderMods?: number;
    /** The library archive now: inside the mods folder (default) or elsewhere. */
    archive: { path: string | null; isDefault: boolean } | null;
    /** Path separator to show (`\` on Windows). */
    sep?: string;
    /** Why folder changes are off right now (another change is running), or "". */
    busy?: string;
    /** Why organizing can't start right now (a scan, another change, an archive move), or "". */
    organizeBlocked?: string;
    onChooseModsFolder: () => void | Promise<void>;
    onCreateModsFolder: () => void | Promise<void>;
    onUseGameFolder: () => void | Promise<void>;
    onRetry: () => void | Promise<void>;
    onPointGameHere: () => void | Promise<void>;
    onChooseArchive: () => Promise<ArchiveChoice>;
    /** Back to the default archive inside the mods folder; resolves a problem, or null. */
    onUseDefaultArchive: () => Promise<string | null>;
    /** The dry run for every loose zip (nothing moves). */
    onPlan: () => Promise<PlannedMove[]>;
    /** File the planned mods, reporting progress; `autoFile` turns on filing new mods. */
    onOrganize: (
      techNames: string[],
      autoFile: boolean,
      onProgress: (done: number, total: number) => void,
    ) => Promise<OrganizeOutcome>;
    onCopyDetails: (text: string) => void;
    /** Leave the setup (the app stores the flag and shows the Library). */
    onfinish: (how: FinishHow) => void;
  }

  let {
    modsStatus,
    gameFound,
    cloudSynced = false,
    scanning,
    scanned,
    scanFailed = false,
    onRescan,
    autoFileOn = false,
    progress,
    modCount,
    activeCount,
    folderMods = 0,
    archive,
    sep = "\\",
    busy = "",
    organizeBlocked = "",
    onChooseModsFolder,
    onCreateModsFolder,
    onUseGameFolder,
    onRetry,
    onPointGameHere,
    onChooseArchive,
    onUseDefaultArchive,
    onPlan,
    onOrganize,
    onCopyDetails,
    onfinish,
  }: Props = $props();

  const STEPS = ["Your mods", "How Silo keeps them", "Organize"] as const;
  let step = $state(1);
  let heading = $state<HTMLElement | null>(null);
  let host = $state<HTMLElement | null>(null);

  // A modal layer: the app underneath is inert and Esc does nothing (Skip setup is the way out).
  $effect(() => {
    if (!host) return;
    const layer = pushLayer({ host, modal: true });
    return () => layer.pop();
  });

  async function goTo(n: number) {
    step = n;
    await tick();
    heading?.focus();
  }

  // Focus the heading when the setup opens, and again when it's replaced under the user (the
  // folder's situation changed: found, switched, created) and focus fell to the page.
  $effect(() => {
    if (!heading) return;
    const lost = document.activeElement === document.body || document.activeElement === null;
    if (lost || !host?.contains(document.activeElement)) heading.focus();
  });

  // ── Step 1: the mods folder ──
  const variant = $derived(folderVariant(modsStatus));
  const path = $derived(modsStatus?.path ?? "");
  /** One action at a time from this screen (a folder switch rescans). */
  let acting = $state(false);
  async function act(fn: () => void | Promise<void>) {
    if (acting) return;
    acting = true;
    try {
      await fn();
    } finally {
      acting = false;
    }
  }

  // ── Step 2: where parked mods live, and auto-file ──
  type Where = "inside" | "other";
  const archiveWhere = (): Where => (archive && !archive.isDefault ? "other" : "inside");
  let where = $state<Where>(archiveWhere());
  let archiveProblem = $state("");
  // svelte-ignore state_referenced_locally
  let autoFile = $state(autoFileOn);
  // Follow the archive Silo actually uses (it loads after startup, and changes when a choice
  // here goes through), except while a choice is being made.
  $effect(() => {
    const now = archiveWhere();
    if (!acting) where = now;
  });
  const insidePath = $derived(path ? `${path}${sep}archive` : `mods${sep}archive`);
  const otherPath = $derived(archive && !archive.isDefault ? archive.path : null);

  async function chooseWhere(next: Where) {
    archiveProblem = "";
    if (next === "other") {
      const before: Where = otherPath ? "other" : "inside";
      const got = await onChooseArchive();
      if (!got) where = before;
      else if ("problem" in got) {
        archiveProblem = got.problem;
        where = before;
      }
    } else if (otherPath) {
      const problem = await onUseDefaultArchive();
      if (problem) {
        archiveProblem = problem;
        where = "other";
      }
    }
  }

  // ── Step 3: the dry run, then filing ──
  let plan = $state<PlannedMove[] | null>(null);
  let planError = $state("");
  let planning = $state(false);
  let filing = $state<{ done: number; total: number } | null>(null);
  /** Why the last try filed nothing (the game is running…). */
  let organizeProblem = $state("");
  let filedCount = $state(0);
  let failures = $state<FilingFailure[] | null>(null);
  let showFiles = $state(false);

  async function loadPlan() {
    planning = true;
    planError = "";
    try {
      plan = await onPlan();
    } catch (e) {
      planError = e instanceof Error ? e.message : String(e);
    } finally {
      planning = false;
    }
  }

  // Plan once the scan has read the folder (it starts at launch, in the background).
  $effect(() => {
    if (step === 3 && scanned && !scanning && plan === null && !planning && !planError) void loadPlan();
  });

  async function organize() {
    if (!plan || filing) return;
    const total = plan.length;
    organizeProblem = "";
    filing = { done: 0, total };
    try {
      const out = await onOrganize(
        plan.map((p) => p.techName),
        autoFile,
        (done, t) => (filing = { done, total: t }),
      );
      if (out.refused) organizeProblem = out.message ?? "";
      else {
        filedCount = out.filed;
        failures = out.failures;
      }
    } finally {
      filing = null;
    }
    await tick();
    heading?.focus();
  }

  const filed = $derived(failures ? filedCount : 0);
  const failedSet = $derived(new Set((failures ?? []).map((f) => f.techName).filter(Boolean)));
</script>

<div class="onboarding" bind:this={host} use:portal role="dialog" aria-modal="true" aria-label="Set up Silo">
  <header class="top">
    <SiloMark variant="wordmark" size={28} />
    {#if !(failures !== null && step === 3)}
      <Button variant="link" disabled={!!filing} onclick={() => onfinish("skip")}>Skip setup</Button>
    {/if}
  </header>

  <ol class="steps" aria-label="Setup steps">
    {#each STEPS as label, i (label)}
      <li class:current={step === i + 1} class:done={step > i + 1} aria-current={step === i + 1 ? "step" : undefined}>
        <span class="dot" aria-hidden="true">{#if step > i + 1}<Icon name="check" size={14} />{:else}{i + 1}{/if}</span>
        <span class="label">{label}</span>{#if step > i + 1}<span class="sr-only"> (done)</span>{/if}
      </li>
    {/each}
  </ol>

  <div class="body">
    {#if step === 1}
      {#if variant === "unavailable"}
        <h2 bind:this={heading} tabindex="-1">Silo can't open your mods folder</h2>
        <p>Silo can't open <code>{path}</code>. Is the drive connected?</p>
        <div class="actions">
          <Button variant="primary" disabled={acting || !!busy} onclick={() => act(onRetry)}>Try again</Button>
          <Button disabled={acting || !!busy} onclick={() => act(onChooseModsFolder)}>Choose another…</Button>
          <Button disabled={acting || !!busy} onclick={() => act(onUseGameFolder)}>Use the game's folder</Button>
        </div>
      {:else if variant === "layout"}
        <h2 bind:this={heading} tabindex="-1">Silo won't change files in this folder</h2>
        <p><code>{path}</code></p>
        <p>{modsStatus?.layoutProblem}</p>
        <div class="actions">
          {#if modsStatus?.layoutProblemCause === "readError"}
            <Button variant="primary" disabled={acting || !!busy} onclick={() => act(onRetry)}>Try again</Button>
          {/if}
          <Button disabled={acting || !!busy} onclick={() => act(onChooseModsFolder)}>Choose another…</Button>
        </div>
      {:else if variant === "missing"}
        <h2 bind:this={heading} tabindex="-1">Farming Simulator 25 hasn't made its mods folder yet</h2>
        <p>Silo can create it at <code>{path}</code>, or you can choose another folder.</p>
        <div class="actions">
          <Button variant="primary" disabled={acting || !!busy} onclick={() => act(onCreateModsFolder)}>Create it</Button>
          <Button disabled={acting || !!busy} onclick={() => act(onChooseModsFolder)}>Choose a folder…</Button>
        </div>
      {:else if variant === "unknown"}
        <h2 bind:this={heading} tabindex="-1">Looking for your mods folder…</h2>
        <p class="muted">If this takes a while, choose the folder yourself.</p>
        <div class="actions">
          <Button disabled={acting || !!busy} onclick={() => act(onRetry)}>Try again</Button>
          <Button disabled={acting || !!busy} onclick={() => act(onChooseModsFolder)}>Choose a folder…</Button>
        </div>
      {:else}
        <h2 bind:this={heading} tabindex="-1">Silo found your mods folder</h2>
        {#if variant === "override"}
          <p>Farming Simulator 25 is set to read <code>{path}</code>. Silo will use that folder too.</p>
        {:else}
          <p><code>{path}</code> <span class="muted">· Farming Simulator 25 reads this folder</span></p>
        {/if}
        {#if variant === "mismatch"}
          <p class="warn" role="note">
            <Icon name="problem" size={16} />
            <span>
              Farming Simulator 25 reads a different folder{modsStatus?.gameReads ? ` (${modsStatus.gameReads})` : ""}, so mods
              you turn on here won't load in the game.
            </span>
          </p>
        {/if}
        {#if cloudSynced}
          <p class="note" role="note">
            <Icon name="cloud" size={16} />
            <span>
              This folder is synced by a cloud service, so Silo will copy active mods instead of linking them. That
              works, but uses more disk space.
            </span>
          </p>
        {/if}
        <p class="tnum">
          {#if scanning && progress.total > 0}
            Reading {formatCount(progress.done)} of {nMods(progress.total)}…
          {:else if scanning}
            Reading your mods…
          {:else if scanFailed}
            Silo couldn't read this folder.
          {:else if scanned}
            Found {nMods(modCount)}.
          {/if}
        </p>
        <p class="sr-only" aria-live="polite">{scanned && !scanning ? `Silo found ${nMods(modCount)} in this folder.` : ""}</p>
        <div class="actions">
          <Button variant="primary" disabled={acting || !folderUsable(variant)} onclick={() => goTo(2)}>
            This is right
          </Button>
          <Button disabled={acting || !!busy} onclick={() => act(onChooseModsFolder)}>Choose another folder…</Button>
          {#if variant === "mismatch" && modsStatus?.canWriteGameSetting}
            <Button disabled={acting || !!busy} onclick={() => act(onPointGameHere)}>Point FS25 here</Button>
          {/if}
        </div>
      {/if}
      {#if !gameFound}
        <p class="muted small">
          Silo didn't find Farming Simulator 25. You can still manage mods; Play appears once the game is found.
        </p>
      {/if}
    {:else if step === 2}
      <h2 bind:this={heading} tabindex="-1">Active mods load in the game. Parked mods wait on the shelf.</h2>
      <div class="shelves" aria-hidden="true">
        <div class="shelf active">
          <Icon name="play" size={20} />
          <strong>Active</strong>
          <span>In the game's mods folder. Loads in the game.</span>
        </div>
        <div class="shelf parked">
          <Icon name="library" size={20} />
          <strong>Parked</strong>
          <span>Kept by Silo. Not loaded.</span>
        </div>
      </div>
      <p>Parking a mod never deletes it. Switch it back on any time.</p>
      <RadioGroup
        legend="Where should parked mods live?"
        bind:value={where}
        disabled={acting || !!busy}
        onchange={(v) => act(() => chooseWhere(v))}
        options={[
          { value: "inside", label: "Inside your mods folder (recommended)", hint: insidePath },
          {
            value: "other",
            label: "On another drive…",
            hint: otherPath ?? "Good for big libraries on a small SSD.",
          },
        ]}
      />
      {#if archiveProblem}<p class="warn" role="alert"><Icon name="problem" size={16} /><span>{archiveProblem}</span></p>{/if}
      <Checkbox bind:checked={autoFile} label="File new mods automatically when they appear in your mods folder" />
      <p class="muted small">Filing starts once you organize in the next step.</p>
      <div class="actions">
        <Button variant="primary" disabled={acting} onclick={() => goTo(3)}>Continue</Button>
        <Button variant="ghost" disabled={acting} onclick={() => goTo(1)}>Back</Button>
      </div>
    {:else if failures !== null}
      {#if failures.length === 0}
        <h2 bind:this={heading} tabindex="-1">Your library is organized</h2>
        <p>
          {nMods(filed)} filed, {formatCount(activeCount)} active, nothing deleted.
        </p>
      {:else}
        <h2 bind:this={heading} tabindex="-1">Silo filed {formatCount(filed)} of {nMods(plan?.length ?? 0)}.</h2>
        <p>{formatCount(failures.length)} couldn't be moved:</p>
        <ul class="fail-list">
          {#each failures as f, i (i)}
            <li><span class="fn">{f.fileName || f.techName || "A mod"}</span>: {f.message}</li>
          {/each}
        </ul>
        <p class="muted">The rest are in your library. A partly organized library works as usual.</p>
      {/if}
      <div class="actions">
        <Button variant="primary" onclick={() => onfinish("done")}>Go to my library</Button>
        {#if failures.length > 0}
          <Button icon="copy" onclick={() => onCopyDetails(failureDetails(failures ?? [], filed, plan?.length ?? 0))}>
            Copy details
          </Button>
        {/if}
      </div>
    {:else if planError}
      <h2 bind:this={heading} tabindex="-1">Silo couldn't work out what to organize</h2>
      <p>{planError}</p>
      <div class="actions">
        <Button variant="primary" onclick={loadPlan}>Try again</Button>
        <Button variant="ghost" onclick={() => onfinish("notNow")}>Not now, show my library</Button>
      </div>
    {:else if plan === null && scanFailed && !scanning}
      <h2 bind:this={heading} tabindex="-1">Silo couldn't read your mods folder</h2>
      <p>The notice behind this screen has the details. Try again, or organize later from the Library.</p>
      <div class="actions">
        {#if onRescan}<Button variant="primary" onclick={() => onRescan?.()}>Try again</Button>{/if}
        <Button variant="ghost" onclick={() => onfinish("notNow")}>Not now, show my library</Button>
      </div>
    {:else if plan === null}
      <h2 bind:this={heading} tabindex="-1">Organize when you're ready</h2>
      <p class="tnum">
        {#if scanning && progress.total > 0}
          Reading {formatCount(progress.done)} of {nMods(progress.total)}…
        {:else}
          Working out what would move…
        {/if}
      </p>
    {:else if plan.length === 0}
      <h2 bind:this={heading} tabindex="-1">Nothing to organize yet</h2>
      <p>Your mods folder has no loose mod zips to file. Mods you add later show up in your library.</p>
      <div class="actions">
        <Button variant="primary" onclick={() => onfinish("notNow")}>Go to my library</Button>
      </div>
    {:else}
      <h2 bind:this={heading} tabindex="-1" class="sr-only">Organize when you're ready</h2>
      <OrganizeSummary {plan} {activeCount} {folderMods} {cloudSynced} heading={false} />
      <button class="disclose" type="button" aria-expanded={showFiles} onclick={() => (showFiles = !showFiles)}>
        <Icon name={showFiles ? "chevronDown" : "chevronRight"} size={16} />
        {showFiles ? "Hide the files" : "See every file"}
      </button>
      {#if showFiles}
        <div class="filelist"><OrganizeFiles {plan} failed={failedSet} /></div>
      {/if}
      {#if filing}
        {@const bar = filingProgress(filing.done, filing.total, scanning ? progress : null)}
        <ProgressBar value={bar.value} max={bar.max} label={bar.label} />
      {:else}
        {#if organizeProblem || organizeBlocked}
          <p class="warn" role={organizeProblem ? "alert" : "status"}>
            <Icon name="problem" size={16} /><span>{organizeProblem || organizeBlocked}</span>
          </p>
        {/if}
        <div class="actions">
          <Button
            variant="primary"
            disabled={!!organizeBlocked}
            title={organizeBlocked || undefined}
            onclick={organize}
          >
            Organize {nMods(plan.length)}
          </Button>
          <Button variant="ghost" onclick={() => onfinish("notNow")}>Not now, show my library</Button>
        </div>
      {/if}
    {/if}
  </div>
</div>

<style>
  .onboarding {
    position: fixed;
    inset: 0;
    z-index: calc(var(--z-scrim) - 1);
    display: flex;
    flex-direction: column;
    overflow-y: auto;
    background: var(--bg);
    color: var(--text);
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--sp-4) var(--sp-5);
  }
  .steps {
    display: flex;
    justify-content: center;
    gap: var(--sp-5);
    list-style: none;
    margin: var(--sp-3) 0 0;
    padding: 0 var(--sp-5);
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .steps li {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .steps .dot {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.5rem;
    height: 1.5rem;
    border-radius: 50%;
    border: 1px solid var(--border-strong);
    font-weight: 600;
  }
  .steps li.current {
    color: var(--text);
    font-weight: 600;
  }
  .steps li.current .dot {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
  }
  .steps li.done .dot {
    border-color: var(--primary);
    color: var(--primary-text);
  }
  .body {
    width: min(40rem, 100% - 2 * var(--sp-5));
    margin: var(--sp-7) auto var(--sp-8);
    display: flex;
    flex-direction: column;
    gap: var(--sp-3);
  }
  h2 {
    margin: 0 0 var(--sp-2);
    font-family: var(--font-display);
    font-size: var(--fs-2xl);
    line-height: var(--lh-2xl);
    font-weight: 600;
    outline: none;
  }
  p {
    margin: 0;
    line-height: 1.55;
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    background: var(--sunken);
    padding: 1px 6px;
    border-radius: var(--r-sm);
    overflow-wrap: anywhere;
  }
  .muted {
    color: var(--muted);
  }
  .small {
    font-size: var(--fs-sm);
  }
  .warn,
  .note {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--r-md);
    font-size: var(--fs-sm);
  }
  .warn {
    background: var(--warn-soft);
    color: var(--warn-text);
  }
  .note {
    background: var(--info-soft);
  }
  .warn :global(svg),
  .note :global(svg) {
    flex: 0 0 auto;
    margin-top: 2px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
    margin-top: var(--sp-2);
  }
  .shelves {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--sp-3);
  }
  .shelf {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    padding: var(--sp-4);
    border: 1px solid var(--border);
    border-bottom-width: 4px;
    border-radius: var(--r-md);
    background: var(--surface);
    font-size: var(--fs-sm);
  }
  .shelf.active {
    border-bottom-color: var(--primary);
  }
  .shelf.parked {
    border-bottom-color: var(--earth);
  }
  .shelf strong {
    font-size: var(--fs-md);
  }
  .disclose {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    border: none;
    background: none;
    padding: 0;
    font: inherit;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--primary-text);
    cursor: pointer;
  }
  .filelist {
    max-height: 18rem;
    overflow-y: auto;
  }
  .fail-list {
    margin: 0;
    padding-left: var(--sp-5);
    max-height: 14rem;
    overflow-y: auto;
    font-size: var(--fs-sm);
  }
  .fn {
    font-weight: 600;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
