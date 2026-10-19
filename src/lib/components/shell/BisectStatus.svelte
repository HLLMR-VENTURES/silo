<script lang="ts">
  // Header mini-status while Find the problem mod runs; it stands in for the status chip so the
  // user can move around Silo without losing the run:
  //   <BisectStatus round={2} onopen={() => go("diagnose", "lastRun")} />
  // `round` is null before the first test round is applied.
  import { bisectStatusText } from "./status";

  interface Props {
    round: number | null;
    onopen: () => void;
  }

  let { round, onopen }: Props = $props();

  const text = $derived(bisectStatusText(round));
</script>

<div class="mini" role="group" aria-label="Find the problem mod">
  <span class="dot" aria-hidden="true"></span>
  <span class="text tnum" role="status">{text}</span>
  <button type="button" class="open" onclick={onopen}>Open</button>
</div>

<style>
  .mini {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 2rem;
    padding: 0 var(--sp-1) 0 var(--sp-3);
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    background: var(--surface);
    font-size: var(--fs-sm);
    white-space: nowrap;
  }
  .dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: var(--r-pill);
    background: var(--info);
    flex: none;
  }
  .open {
    min-height: 1.5rem;
    padding: 0 var(--sp-2);
    border: none;
    border-radius: var(--r-pill);
    background: transparent;
    color: var(--info);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
    text-decoration: underline;
  }
  .open:hover {
    background: var(--sunken);
  }
</style>
