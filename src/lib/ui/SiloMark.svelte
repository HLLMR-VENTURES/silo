<script lang="ts">
  // The silo mark: <SiloMark variant="tile" size={40} />. Same drawing as src/assets/brand/*.svg,
  // inlined so it needs no fetch. Brand colors are fixed hex (the mark looks the same in both
  // themes); only the wordmark text follows the theme (--text, Fraunces).
  // `size` is the height in px at the default UI size; it renders in rem so it follows UI size.

  interface Props {
    variant?: "tile" | "glyph" | "wordmark";
    size?: number;
    /** Glyph and wordmark only: draw everything in currentColor (single-color use). */
    mono?: boolean;
    /** Accessible name; `false` hides the mark from assistive tech (when text beside it names it). */
    label?: string | false;
    class?: string;
  }

  let { variant = "tile", size = 24, mono = false, label = "Silo", class: className = "" }: Props =
    $props();

  const uid = $props.id();
  const rem = $derived(`${size / 16}rem`);
  const siloFill = $derived(mono ? "currentColor" : "#4A7330");
  const sunFill = $derived(mono ? "currentColor" : "#E3B23C");
  const role = $derived(label === false ? undefined : "img");
  const ariaLabel = $derived(label === false ? undefined : label);
  const ariaHidden = $derived(label === false ? ("true" as const) : undefined);
</script>

{#snippet glyph(hidden: boolean)}
  <svg
    class={hidden ? "silo-mark-glyph" : `silo-mark ${className}`}
    viewBox="0 0 32 32"
    style="width: {rem}; height: {rem}"
    focusable="false"
    role={hidden ? undefined : role}
    aria-label={hidden ? undefined : ariaLabel}
    aria-hidden={hidden ? "true" : ariaHidden}
  >
    <defs>
      <mask id="{uid}-sun" maskUnits="userSpaceOnUse" x="0" y="0" width="32" height="32">
        <rect width="32" height="32" fill="#fff" />
        <path
          d="M5 29V12a6 6 0 0 1 12 0v17zM9.5 3h3v4h-3z"
          fill="#000"
          stroke="#000"
          stroke-width="4"
          stroke-linejoin="round"
        />
      </mask>
    </defs>
    <circle cx="21.5" cy="10" r="6.5" fill={sunFill} mask="url(#{uid}-sun)" />
    <rect x="9.5" y="3" width="3" height="3.5" rx="0.75" fill={siloFill} />
    <path d="M5 19v-7a6 6 0 0 1 12 0v7zM5 21h12v3H5zM5 26h12v3H5z" fill={siloFill} />
  </svg>
{/snippet}

{#if variant === "tile"}
  <svg
    class="silo-mark {className}"
    viewBox="0 0 64 64"
    style="width: {rem}; height: {rem}"
    focusable="false"
    {role}
    aria-label={ariaLabel}
    aria-hidden={ariaHidden}
  >
    <defs>
      <linearGradient id="{uid}-bg" x1="0" y1="0" x2="0" y2="1">
        <stop offset="0" stop-color="#4A7330" />
        <stop offset="1" stop-color="#395A25" />
      </linearGradient>
    </defs>
    <rect width="64" height="64" rx="14" fill="url(#{uid}-bg)" />
    <circle cx="44" cy="25" r="12" fill="#E3B23C" />
    <rect x="27" y="11" width="4" height="6" rx="1" fill="#F6F3EC" />
    <path d="M19 51V26a10 10 0 0 1 20 0v25z" fill="#F6F3EC" />
    <path d="M19 32h20v2H19zM19 39h20v2H19zM19 46h20v2H19z" fill="#E4DECE" />
    <rect x="15" y="51" width="28" height="3" rx="1.5" fill="#E4DECE" />
  </svg>
{:else if variant === "glyph"}
  {@render glyph(false)}
{:else}
  <span
    class="silo-wordmark {className}"
    style="font-size: {(size * 0.75) / 16}rem"
    {role}
    aria-label={ariaLabel}
    aria-hidden={ariaHidden}
  >
    {@render glyph(true)}
    <span class="silo-wordmark-text">Silo</span>
  </span>
{/if}

<style>
  .silo-mark,
  .silo-mark-glyph {
    display: inline-block;
    flex: none;
    vertical-align: middle;
  }
  .silo-wordmark {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    line-height: 1;
  }
  .silo-wordmark-text {
    font-family: var(--font-display);
    font-weight: 600;
    font-variation-settings: "SOFT" 50;
    color: var(--text);
  }
</style>
