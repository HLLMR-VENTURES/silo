<script lang="ts">
  // Header status: <StatusChip problems={2} review={3} onclick={() => go("diagnose")} />.
  // "All good" (green dot), "N problems" (red), or "N to review" (amber); problems win. Pass only
  // problem and review counts: info findings never count.
  import { statusChipModel } from "./status";

  interface Props {
    problems: number;
    review: number;
    onclick?: (e: MouseEvent) => void;
  }

  let { problems, review, onclick }: Props = $props();

  const model = $derived(statusChipModel(problems, review));
</script>

<button type="button" class="chip tone-{model.tone}" aria-label={model.label} data-tone={model.tone} {onclick}>
  <span class="dot" aria-hidden="true"></span>
  <span class="text tnum">{model.text}</span>
</button>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 2rem;
    padding: 0 var(--sp-3);
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-sm);
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition: background-color var(--dur-1) var(--ease);
  }
  .chip:hover {
    background: var(--sunken);
  }
  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: var(--r-pill);
    flex: none;
  }
  .tone-ok .dot {
    background: var(--success);
  }
  .tone-problem .dot {
    background: var(--danger);
  }
  .tone-problem .text {
    color: var(--danger);
  }
  .tone-review .dot {
    background: var(--warn-text);
  }
  .tone-review .text {
    color: var(--warn-text);
  }
  @media (prefers-reduced-motion: reduce) {
    .chip {
      transition: none;
    }
  }
</style>
