<script lang="ts">
  // Test-only: AppShell with simple header, notices, content and an optional drawer.
  import AppShell from "../AppShell.svelte";
  import AppHeader from "../AppHeader.svelte";
  import ViewHeader from "../ViewHeader.svelte";
  import type { RailPin } from "../../../state/nav.svelte";

  let {
    withDrawer = false,
    withNotices = true,
    pin,
    wide = false,
  }: { withDrawer?: boolean; withNotices?: boolean; pin?: RailPin; wide?: boolean } = $props();
</script>

{#snippet header()}
  <AppHeader>
    {#snippet activity()}<button type="button">Bell</button>{/snippet}
    {#snippet play()}<button type="button">Play</button>{/snippet}
  </AppHeader>
{/snippet}

{#snippet notices()}<p>A notice</p>{/snippet}

{#snippet drawer()}<p>Drawer body</p>{/snippet}

<AppShell {header} notices={withNotices ? notices : undefined} drawer={withDrawer ? drawer : undefined} {pin} {wide}>
  <ViewHeader title="Library" description="Your mods." />
  <button type="button">In content</button>
</AppShell>
