<script lang="ts">
  // Test-only: ViewOutlet following nav.view, with a stub snippet per view (Help left out).
  import ViewOutlet from "../ViewOutlet.svelte";
  import { nav } from "../../state/nav.svelte";
</script>

<ViewOutlet view={nav.view}>
  {#snippet library()}<p data-testid="view">library</p>{/snippet}
  {#snippet browse()}<p data-testid="view">browse</p>{/snippet}
  {#snippet modSets()}<p data-testid="view">modSets</p>{/snippet}
  {#snippet diagnose()}<p data-testid="view">diagnose</p>{/snippet}
  {#snippet updates()}<p data-testid="view">updates</p>{/snippet}
  {#snippet tools()}<p data-testid="view">tools</p>{/snippet}
  {#snippet settings()}<p data-testid="view">settings</p>{/snippet}
</ViewOutlet>
