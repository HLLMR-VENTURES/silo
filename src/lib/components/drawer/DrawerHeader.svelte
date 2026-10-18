<script lang="ts">
  // The drawer's header: icon, title, author · version, the verdict badges and Close.
  // The title takes focus when the drawer opens (and when it switches to another mod), so a
  // screen reader announces the mod and Tab starts at the top: call focusTitle().
  import BadgeRow from "../library/BadgeRow.svelte";
  import IconButton from "../../ui/IconButton.svelte";
  import type { BadgeSpec } from "../library/rowBadges";
  import { versionLabel } from "./drawerText";

  interface Props {
    title: string;
    author: string | null;
    version: string | null;
    icon: string | null;
    badges: BadgeSpec[];
    onClose: () => void;
  }

  let { title, author, version, icon, badges, onClose }: Props = $props();

  let titleEl = $state<HTMLElement | null>(null);
  const spoken = $derived(badges.map((b) => `${b.label}: ${b.title.replace(/\.$/, "")}.`).join(" "));

  /** Move focus to the title (it isn't in the tab order: tabindex -1). */
  export function focusTitle(): void {
    titleEl?.focus();
  }
</script>

<div class="head">
  {#if icon}<img class="icon" src={icon} alt="" />{/if}
  <div class="text">
    <h2 class="title" tabindex="-1" bind:this={titleEl}>{title}</h2>
    <div class="sub">
      {author ?? "Unknown author"}{version ? ` · ${versionLabel(version)}` : ""}
    </div>
    {#if badges.length > 0}
      <!-- The pills' explanations are hover tooltips; screen readers get them as text instead. -->
      <div class="badges" aria-hidden="true"><BadgeRow {badges} /></div>
      <p class="vh">{spoken}</p>
    {/if}
  </div>
  <IconButton icon="close" label="Close details" onclick={onClose} />
</div>

<style>
  .head {
    display: flex;
    gap: var(--sp-3);
    align-items: flex-start;
  }
  .icon {
    width: 44px;
    height: 44px;
    flex: none;
    object-fit: contain;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
  }
  .text {
    flex: 1 1 auto;
    min-width: 0;
  }
  .title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    font-weight: 600;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }
  /* Focus lands here programmatically; the ring only shows for keyboard users (focus-visible). */
  .title:focus {
    outline: none;
  }
  .title:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
    border-radius: var(--radius-sm);
  }
  .sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-top: 2px;
  }
  .vh {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: 0;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .badges {
    margin-top: var(--sp-2);
    display: flex;
  }
</style>
