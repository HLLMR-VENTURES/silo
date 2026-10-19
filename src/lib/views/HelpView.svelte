<script lang="ts" module>
  import { BUG_REPORT_URL, DISCORD_URL, DISCUSSIONS_URL } from "../links";
  export { BUG_REPORT_URL, DISCORD_URL, DISCUSSIONS_URL };
</script>

<script lang="ts">
  // Help (#250): help topics with search ("loadouts" finds Mod sets), What's new, Keyboard
  // shortcuts, where to ask, Report a problem (copies diagnostics, then opens the bug form), and
  // About with Silo's license and the third-party licenses.
  //   <HelpView {appVer} onopen={(url) => openExternal(url)} oncopydiagnostics={copyDiagnostics}
  //     onshowshortcuts={() => (shortcutsOpen = true)} onshowwhatsnew={() => (whatsNewOpen = true)} />
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import Button from "../ui/Button.svelte";
  import Kbd from "../ui/Kbd.svelte";
  import TextField from "../ui/TextField.svelte";
  import { searchTopics } from "../help/topics";
  import { THIRD_PARTY } from "../help/licenses";
  import { CHANGELOG_URL, LICENSE_URL, RELEASES_URL, SOURCE_URL, WEBSITE_URL } from "../links";

  interface Props {
    /** Open a link in the browser. */
    onopen: (url: string) => void;
    /** Copy Silo's diagnostics for a bug report (Report a problem waits for it). */
    oncopydiagnostics: () => void | Promise<void>;
    /** Open the Keyboard shortcuts dialog (the same one ? opens). */
    onshowshortcuts?: () => void;
    /** Open the "What's new in Silo 1.0" card again. */
    onshowwhatsnew?: () => void;
    /** Silo's version, for About. */
    appVer?: string | null;
  }

  let { onopen, oncopydiagnostics, onshowshortcuts, onshowwhatsnew, appVer = null }: Props = $props();

  let query = $state("");
  const topics = $derived(searchTopics(query));
  let reporting = $state(false);

  /** Copy the diagnostics first, then open the form, so they're ready to paste. */
  async function reportProblem() {
    if (reporting) return;
    reporting = true;
    try {
      await oncopydiagnostics();
    } catch {
      // The copy reports its own problem; the form still opens.
    } finally {
      reporting = false;
    }
    onopen(BUG_REPORT_URL);
  }
</script>

<ViewBoundary name="Help" bodyText="Your mods weren't changed.">
  <ViewHeader title="Help" />
  <div class="help">
    <section aria-labelledby="help-topics">
      <h3 id="help-topics">Help topics</h3>
      <TextField
        type="search"
        bind:value={query}
        placeholder="Search help"
        aria-label="Search help"
        class="search"
      />
      {#if topics.length === 0}
        <p class="muted">Nothing in Help matches “{query}”. Try another word, or ask on Discord.</p>
      {:else}
        <ul class="topics">
          {#each topics as t (t.id)}
            <li>
              <details open={query.trim() !== "" && topics.length <= 3}>
                <summary>{t.title}</summary>
                <p>{t.body}</p>
              </details>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if onshowwhatsnew}
      <section aria-labelledby="help-new">
        <h3 id="help-new">What's new</h3>
        <p>A short tour of what changed in Silo 1.0.</p>
        <div class="row">
          <Button onclick={onshowwhatsnew}>See what's new</Button>
          <Button variant="ghost" icon="external" onclick={() => onopen(CHANGELOG_URL)}>Full changelog</Button>
        </div>
      </section>
    {/if}

    {#if onshowshortcuts}
      <section aria-labelledby="help-keys">
        <h3 id="help-keys">Keyboard shortcuts</h3>
        <p>
          <Kbd>Ctrl</Kbd>+<Kbd>K</Kbd> searches, <Kbd>F5</Kbd> rescans your library,
          <Kbd>Alt</Kbd>+<Kbd>1</Kbd>…<Kbd>5</Kbd> switch pages and <Kbd>F6</Kbd> moves between the
          sidebar, header and page. Press <Kbd>?</Kbd> anywhere for the full list.
        </p>
        <div class="row">
          <Button onclick={onshowshortcuts}>Keyboard shortcuts</Button>
        </div>
      </section>
    {/if}

    <section aria-labelledby="help-ask">
      <h3 id="help-ask">Ask a question</h3>
      <p>
        Questions and ideas go to GitHub Discussions. For quick help and release news, join the
        Silo Discord.
      </p>
      <div class="row">
        <Button icon="external" onclick={() => onopen(DISCUSSIONS_URL)}>GitHub Discussions</Button>
        <Button icon="external" onclick={() => onopen(DISCORD_URL)}>Discord</Button>
      </div>
    </section>

    <section aria-labelledby="help-report">
      <h3 id="help-report">Report a problem</h3>
      <p>
        Silo copies its diagnostics (a summary of your library and Silo's recent log, with your
        user name removed), then opens the bug report form on GitHub. Paste them in where it asks.
      </p>
      <div class="row">
        <Button variant="primary" icon="bug" loading={reporting} onclick={reportProblem}>Report a problem</Button>
        <Button icon="copy" onclick={() => oncopydiagnostics()}>Copy diagnostics only</Button>
      </div>
    </section>

    <section aria-labelledby="help-about">
      <h3 id="help-about">About Silo</h3>
      <p class="tnum">Silo {appVer ?? ""}</p>
      <p>
        Silo is free and open source under the MIT License. Copyright © 2026 HLLMR Ventures LLC.
        Silo isn't affiliated with GIANTS Software. Farming Simulator is a trademark of its
        respective owner.
      </p>
      <div class="row">
        <Button variant="ghost" icon="external" onclick={() => onopen(WEBSITE_URL)}>Website</Button>
        <Button variant="ghost" icon="external" onclick={() => onopen(SOURCE_URL)}>Source code</Button>
        <Button variant="ghost" icon="external" onclick={() => onopen(LICENSE_URL)}>License</Button>
      </div>
      <details class="licenses">
        <summary>Third-party licenses</summary>
        <p class="muted">
          Silo's window uses these fonts and packages. Every Rust crate and npm package Silo is built
          from, with its license, is listed in the software bill of materials attached to each
          release.
        </p>
        <Button variant="link" onclick={() => onopen(RELEASES_URL)}>Releases on GitHub</Button>
        <ul class="lic-list">
          {#each THIRD_PARTY as l (l.name)}
            <li>
              <details>
                <summary>
                  <span class="lic-name">{l.name}</span>{#if l.version}{" "}<span class="tnum muted">{l.version}</span>{/if}
                  <span class="lic-id">{l.license}</span>
                </summary>
                <pre>{l.text}</pre>
              </details>
            </li>
          {/each}
        </ul>
      </details>
    </section>
  </div>
</ViewBoundary>

<style>
  .help {
    display: flex;
    flex-direction: column;
    gap: var(--sp-6);
    max-width: 48rem;
  }
  h3 {
    font-size: var(--fs-lg);
    margin: 0 0 var(--sp-2);
  }
  p {
    margin: 0 0 var(--sp-3);
    color: var(--muted);
    max-width: 65ch;
  }
  .muted {
    color: var(--muted);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  :global(.help .search) {
    width: 100%;
    max-width: 28rem;
    margin-bottom: var(--sp-3);
  }
  .topics {
    list-style: none;
    margin: 0;
    padding: 0;
    border-top: 1px solid var(--border);
  }
  .topics li {
    border-bottom: 1px solid var(--border);
  }
  .topics summary {
    cursor: pointer;
    padding: var(--sp-2) 0;
    font-weight: 600;
  }
  .topics p {
    margin: 0 0 var(--sp-3);
    color: var(--text);
  }
  .licenses {
    margin-top: var(--sp-4);
  }
  .licenses > summary {
    cursor: pointer;
    font-weight: 600;
    margin-bottom: var(--sp-2);
  }
  .lic-list {
    list-style: none;
    margin: var(--sp-3) 0 0;
    padding: 0;
  }
  .lic-list summary {
    cursor: pointer;
    padding: 2px 0;
  }
  .lic-name {
    font-weight: 600;
  }
  .lic-id {
    margin-left: var(--sp-2);
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  pre {
    max-height: 16rem;
    overflow: auto;
    padding: var(--sp-3);
    background: var(--sunken);
    border-radius: var(--r-sm);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    white-space: pre-wrap;
  }
</style>
