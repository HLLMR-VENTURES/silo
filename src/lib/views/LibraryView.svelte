<script lang="ts">
  // Home: the title with its "More" menu (Rescan, Re-read every mod), the status strip, then the
  // filters and the list App renders (they keep App's state and handlers):
  //   <LibraryView stripLeft="Ready to play · 142 of 737 active" {stripItems} {menuItems}
  //     onmenuselect={run}>{#snippet filters()}...{/snippet}{#snippet list()}...{/snippet}</LibraryView>
  // The list fills the rest of the height (AppShell `fill`): VirtualList scrolls itself.
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import StatusStrip, { type StatusStripItem } from "../components/shell/StatusStrip.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import MenuButton from "../ui/MenuButton.svelte";
  import type { MenuItem } from "../ui/overlayTypes";

  interface Props {
    stripLeft: string;
    stripItems?: StatusStripItem[];
    /** The header's "More library actions" menu. */
    menuItems: MenuItem[];
    onmenuselect: (id: string) => void;
    /** FilterBar, the toggle row and LibraryToolbar. */
    filters?: Snippet;
    /** The virtual list, or the empty state. */
    list: Snippet;
  }

  let { stripLeft, stripItems = [], menuItems, onmenuselect, filters, list }: Props = $props();
</script>

<ViewBoundary name="the library">
  <div class="library">
    <ViewHeader title="Library">
      {#snippet actions()}
        <MenuButton
          label="More library actions"
          icon="more"
          iconOnly
          items={menuItems}
          onselect={onmenuselect}
          placement="bottom-end"
        />
      {/snippet}
    </ViewHeader>
    <div class="strip"><StatusStrip left={stripLeft} items={stripItems} /></div>
    {@render filters?.()}
    <div class="list-wrap">{@render list()}</div>
  </div>
</ViewBoundary>

<style>
  .library {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .library :global(.view-header) {
    margin-bottom: var(--sp-2);
  }
  .strip {
    margin-bottom: var(--sp-3);
  }
  .list-wrap {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
  }
</style>
