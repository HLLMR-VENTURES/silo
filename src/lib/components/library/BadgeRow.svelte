<script lang="ts">
  // A mod's badges, capped: <BadgeRow badges={rowBadges(mod, ctx)} /> shows the first three, then
  // "+n" (its tooltip and name list the rest).
  import Badge from "../../ui/Badge.svelte";
  import { capBadges } from "../../ui/badges";
  import { tooltip } from "../../ui/tooltip";
  import type { BadgeSpec } from "./rowBadges";

  let { badges, max = 3 }: { badges: BadgeSpec[]; max?: number } = $props();

  const capped = $derived(capBadges(badges, max));
  const rest = $derived(badges.slice(capped.shown.length).map((b) => b.label).join(", "));
</script>

{#if badges.length > 0}
  <span class="badge-row">
    {#each capped.shown as b (b.id)}
      <span class="tip" use:tooltip={b.title}><Badge tone={b.tone} icon={b.icon}>{b.label}</Badge></span>
    {/each}
    {#if capped.more > 0}
      <span class="tip" use:tooltip={rest}>
        <Badge tone="neutral">+{capped.more}<span class="vh"> more: {rest}</span></Badge>
      </span>
    {/if}
  </span>
{/if}

<style>
  .badge-row {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    min-width: 0;
    overflow: hidden;
  }
  .vh {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .tip {
    display: inline-flex;
    flex: none;
  }
</style>
