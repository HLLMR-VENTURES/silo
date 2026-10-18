<script lang="ts">
  // Progress with visible text: <ProgressBar value={214} max={612} label="214 of 612 mods" />.
  // Leave `value` undefined for indeterminate (animated stripe; static under reduced motion).
  interface Props {
    value?: number;
    max?: number;
    /** Visible text and the bar's accessible name. */
    label: string;
    /** Hide the visible text (it stays the accessible name). */
    hideLabel?: boolean;
    class?: string;
  }

  let { value, max = 100, label, hideLabel = false, class: className = "" }: Props = $props();

  const indeterminate = $derived(value === undefined || Number.isNaN(value));
  const clamped = $derived(indeterminate ? 0 : Math.min(Math.max(value ?? 0, 0), max));
  const pct = $derived(max > 0 ? (clamped / max) * 100 : 0);
</script>

<div class="progress {className}">
  {#if !hideLabel}<div class="label" aria-hidden="true">{label}</div>{/if}
  <div
    class="track"
    class:indeterminate
    role="progressbar"
    aria-label={label}
    aria-valuemin={0}
    aria-valuemax={max}
    aria-valuenow={indeterminate ? undefined : clamped}
  >
    <div class="fill" style:width={indeterminate ? undefined : `${pct}%`}></div>
  </div>
</div>

<style>
  .progress {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  .label {
    color: var(--muted);
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
    font-variant-numeric: tabular-nums;
  }
  .track {
    position: relative;
    height: 0.375rem;
    overflow: hidden;
    border-radius: var(--r-pill);
    background: var(--sunken);
    box-shadow: inset 0 0 0 1px var(--border);
  }
  .fill {
    height: 100%;
    border-radius: inherit;
    background: var(--primary);
    transition: width var(--dur-2) var(--ease);
  }
  .indeterminate .fill {
    position: absolute;
    inset: 0 auto 0 0;
    width: 40%;
    animation: slide 1.2s var(--ease) infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(250%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
    .indeterminate .fill {
      width: 100%;
      animation: none;
      opacity: 0.45;
    }
  }
</style>
