<script lang="ts">
  // Header search: <SearchField bind:value placeholder="Search your mods" onsubmit={run} bind:this={field} />
  // then field.focus(). Esc clears the text; Esc on an empty field leaves it (blur). Enter submits.
  // The Ctrl K hint is display-only; the global shortcut is wired by the app.
  import Icon from "../../ui/Icon.svelte";
  import Kbd from "../../ui/Kbd.svelte";

  interface Props {
    value?: string;
    placeholder?: string;
    /** Accessible name of the input. */
    label?: string;
    /** Show the Ctrl K hint. */
    shortcut?: boolean;
    onsubmit?: (value: string) => void;
    /** After Esc clears the text. */
    onclear?: () => void;
  }

  let {
    value = $bindable(""),
    placeholder = "Search your mods",
    label = "Search",
    shortcut = true,
    onsubmit,
    onclear,
  }: Props = $props();

  let input = $state<HTMLInputElement | null>(null);

  /** Focus the input and select its text. */
  export function focus(): void {
    input?.focus();
    input?.select();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    if (value !== "") {
      value = "";
      onclear?.();
    } else {
      input?.blur();
    }
  }

  function submit(e: SubmitEvent) {
    e.preventDefault();
    onsubmit?.(value);
  }
</script>

<form class="search" role="search" onsubmit={submit}>
  <span class="icon" aria-hidden="true"><Icon name="search" size={16} /></span>
  <input
    bind:this={input}
    bind:value
    type="search"
    class="input"
    {placeholder}
    aria-label={label}
    aria-keyshortcuts={shortcut ? "Control+K" : undefined}
    autocomplete="off"
    spellcheck="false"
    {onkeydown}
  />
  {#if shortcut && value === ""}
    <span class="hint" aria-hidden="true"><Kbd>Ctrl</Kbd><Kbd>K</Kbd></span>
  {/if}
</form>

<style>
  .search {
    position: relative;
    display: flex;
    align-items: center;
    width: 100%;
    min-width: 0;
    margin: 0;
  }
  .icon {
    position: absolute;
    left: var(--sp-3);
    display: inline-flex;
    color: var(--muted);
    pointer-events: none;
  }
  .input {
    width: 100%;
    min-height: 2.25rem;
    padding: 0 4.5rem 0 calc(var(--sp-3) + 16px + var(--sp-2));
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }
  .input::placeholder {
    color: var(--muted);
    opacity: 1;
  }
  .input::-webkit-search-cancel-button,
  .input::-webkit-search-decoration {
    appearance: none;
  }
  .hint {
    position: absolute;
    right: var(--sp-2);
    display: inline-flex;
    gap: 2px;
    pointer-events: none;
  }
</style>
