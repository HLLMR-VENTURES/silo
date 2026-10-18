<script lang="ts">
  // Test-only: AppShell with the app's shortcut listener wired the way App.svelte wires it
  // (focus search, F6 region cycling, ? opens the Keyboard shortcuts dialog); every action is
  // also reported to `onaction`.
  import { get } from "svelte/store";
  import AppShell from "../AppShell.svelte";
  import AppHeader from "../AppHeader.svelte";
  import ShortcutsDialog from "../ShortcutsDialog.svelte";
  import { cycleRegion } from "../regions";
  import { installShortcuts, type ShortcutAction } from "../../../state/shortcuts";
  import { modalOpen } from "../../../ui/overlayStack";

  let {
    noticeText = "A notice",
    withDrawer = true,
    onaction,
  }: { noticeText?: string; withDrawer?: boolean; onaction?: (a: ShortcutAction) => void } = $props();

  let appHeader = $state<AppHeader | null>(null);
  let shortcutsOpen = $state(false);

  function run(a: ShortcutAction) {
    onaction?.(a);
    if (a.kind === "focusSearch") appHeader?.focusSearch();
    else if (a.kind === "region") cycleRegion(document, a.dir, document.activeElement);
    else if (a.kind === "help") shortcutsOpen = true;
  }

  $effect(() => installShortcuts(window, run, () => get(modalOpen)));
</script>

{#snippet header()}
  <AppHeader bind:this={appHeader}>
    {#snippet activity()}<button type="button">Bell</button>{/snippet}
  </AppHeader>
{/snippet}

{#snippet notices()}{#if noticeText}<p>{noticeText}</p>{/if}{/snippet}

{#snippet drawer()}<button type="button">Close details</button>{/snippet}

<AppShell {header} {notices} drawer={withDrawer ? drawer : undefined}>
  <button type="button">In content</button>
</AppShell>
<ShortcutsDialog bind:open={shortcutsOpen} />
