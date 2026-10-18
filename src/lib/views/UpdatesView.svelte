<script lang="ts">
  // Updates: the update list App renders, with "Check for updates" in the header.
  //   <UpdatesView checking={false} checkDisabled={false} oncheck={check}>...</UpdatesView>
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import Button from "../ui/Button.svelte";
  import { tooltip } from "../ui/tooltip";

  interface Props {
    checking: boolean;
    /** The check can't start now (a library write is running, or there are no mods). */
    checkDisabled: boolean;
    /** Why it can't, shown as the button's tooltip. */
    checkDisabledReason?: string;
    oncheck: () => void;
    children: Snippet;
  }

  let { checking, checkDisabled, checkDisabledReason = "", oncheck, children }: Props = $props();
</script>

<ViewBoundary name="Updates">
  <ViewHeader
    title="Updates"
    description="Newer versions of your mods, from the catalog and any GitHub repos you linked."
  >
    {#snippet actions()}
      <span use:tooltip={checkDisabled && checkDisabledReason ? checkDisabledReason : null}>
        <Button icon="update" loading={checking} disabled={checkDisabled} onclick={oncheck}>
          Check for updates
        </Button>
      </span>
    {/snippet}
  </ViewHeader>
  {@render children()}
</ViewBoundary>
