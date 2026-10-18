<script lang="ts">
  // Loading placeholder: <Skeleton variant="line" width="60%" />. Hidden from assistive tech;
  // announce loading elsewhere (e.g. aria-busy on the region). Shimmer stops under reduced motion.
  interface Props {
    variant?: "line" | "block" | "circle";
    /** Any CSS length. Defaults: line 100%, block 100%, circle 2rem. */
    width?: string;
    /** Any CSS length. Defaults: line 1em, block 4rem, circle = width. */
    height?: string;
    class?: string;
  }

  let { variant = "line", width, height, class: className = "" }: Props = $props();

  const w = $derived(width ?? (variant === "circle" ? "2rem" : "100%"));
  const h = $derived(height ?? (variant === "circle" ? w : variant === "block" ? "4rem" : "1em"));
</script>

<span class="skeleton sk-{variant} {className}" style:width={w} style:height={h} aria-hidden="true"
></span>

<style>
  .skeleton {
    display: block;
    background: linear-gradient(
        90deg,
        transparent 0%,
        color-mix(in srgb, var(--raised) 60%, transparent) 50%,
        transparent 100%
      )
      var(--sunken);
    background-size: 200% 100%;
    background-repeat: no-repeat;
    animation: shimmer 1.4s linear infinite;
  }
  .sk-line {
    border-radius: var(--r-sm);
  }
  .sk-block {
    border-radius: var(--r-md);
  }
  .sk-circle {
    border-radius: 50%;
  }

  @keyframes shimmer {
    from {
      background-position: 150% 0;
    }
    to {
      background-position: -50% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .skeleton {
      animation: none;
      background-image: none;
    }
  }
</style>
