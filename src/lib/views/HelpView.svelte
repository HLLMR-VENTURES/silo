<script lang="ts" module>
  export const DISCUSSIONS_URL = "https://github.com/HLLMR/silo/discussions";
  export const DISCORD_URL = "https://discord.gg/NpxcYdGHrW";
  export const NEW_ISSUE_URL = "https://github.com/HLLMR/silo/issues/new/choose";
</script>

<script lang="ts">
  // Help (a placeholder until the full page lands): where to ask, how to report a problem, and the
  // keyboard shortcuts.
  //   <HelpView onopen={(url) => openExternal(url)} oncopydiagnostics={copyDiagnostics}
  //     onshowshortcuts={() => (shortcutsOpen = true)} />
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import Button from "../ui/Button.svelte";
  import Kbd from "../ui/Kbd.svelte";

  interface Props {
    /** Open a link in the browser. */
    onopen: (url: string) => void;
    /** Copy Silo's diagnostics for a bug report. */
    oncopydiagnostics: () => void;
    /** Open the Keyboard shortcuts dialog (the same one ? opens). */
    onshowshortcuts?: () => void;
  }

  let { onopen, oncopydiagnostics, onshowshortcuts }: Props = $props();
</script>

<ViewBoundary name="Help" bodyText="Your mods weren't changed.">
  <ViewHeader title="Help" />
  <div class="help">
    <section>
      <h3>Ask a question</h3>
      <p>
        Questions and ideas go to GitHub Discussions. For quick help and release news, join the
        Silo Discord.
      </p>
      <div class="row">
        <Button icon="external" onclick={() => onopen(DISCUSSIONS_URL)}>GitHub Discussions</Button>
        <Button icon="external" onclick={() => onopen(DISCORD_URL)}>Discord</Button>
      </div>
    </section>
    <section>
      <h3>Report a problem</h3>
      <p>
        Copy Silo's diagnostics (a summary of your library and Silo's recent log, with your user
        name removed), then open an issue and paste them in.
      </p>
      <div class="row">
        <Button icon="copy" onclick={oncopydiagnostics}>Copy diagnostics</Button>
        <Button icon="external" onclick={() => onopen(NEW_ISSUE_URL)}>Report a problem</Button>
      </div>
    </section>
    {#if onshowshortcuts}
      <section>
        <h3>Keyboard shortcuts</h3>
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
  </div>
</ViewBoundary>

<style>
  .help {
    display: flex;
    flex-direction: column;
    gap: var(--sp-5);
    max-width: 48rem;
  }
  h3 {
    font-size: var(--fs-lg);
    margin: 0 0 var(--sp-1);
  }
  p {
    margin: 0 0 var(--sp-3);
    color: var(--muted);
    max-width: 65ch;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
</style>
