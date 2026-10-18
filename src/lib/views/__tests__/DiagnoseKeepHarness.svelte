<script lang="ts">
  // Test-only: App's Diagnose wiring. The view is shown only on nav.view === "diagnose" (as
  // ViewOutlet does), and Last run's content docks into its slot through KeepMounted.
  import DiagnoseView from "../DiagnoseView.svelte";
  import KeepMounted from "../../components/KeepMounted.svelte";
  import BisectProbe from "./BisectProbe.svelte";
  import { nav } from "../../state/nav.svelte";

  let { bisecting = false, onBisecting }: { bisecting?: boolean; onBisecting: (v: boolean) => void } =
    $props();

  let slot = $state<HTMLElement | null>(null);
</script>

{#if nav.view === "diagnose"}
  <DiagnoseView bind:lastRunSlot={slot}>
    {#snippet conflictsTab()}<p>Conflicts body</p>{/snippet}
    {#snippet problemsTab()}<p>Health body</p>{/snippet}
  </DiagnoseView>
{/if}
<KeepMounted target={slot} keep={bisecting}>
  <BisectProbe {onBisecting} />
</KeepMounted>
