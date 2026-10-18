<script lang="ts">
  // A trigger button plus its Menu:
  //   <MenuButton label="Actions" {items} onselect={run} />                 (text button)
  //   <MenuButton label="More actions" icon="more" iconOnly {items} onselect={run} />
  // The trigger carries aria-haspopup="menu" / aria-expanded; ArrowDown or ArrowUp on it opens the menu.
  import Button from "./Button.svelte";
  import IconButton from "./IconButton.svelte";
  import Menu from "./Menu.svelte";
  import type { IconName } from "./icons";
  import type { MenuItem, Placement } from "./overlayTypes";

  interface Props {
    /** Button text, or the accessible name when `iconOnly`. Also names the menu. */
    label: string;
    icon?: IconName;
    /** Render an IconButton (requires `icon`). */
    iconOnly?: boolean;
    variant?: "primary" | "secondary" | "ghost";
    size?: "sm" | "md";
    items: MenuItem[];
    onselect: (id: string) => void;
    placement?: Placement;
    disabled?: boolean;
    open?: boolean;
  }

  let {
    label,
    icon,
    iconOnly = false,
    variant = "secondary",
    size = "md",
    items,
    onselect,
    placement = "bottom-start",
    disabled = false,
    open = $bindable(false),
  }: Props = $props();

  const uid = $props.id();
  const menuId = `${uid}-menu`;
  let wrap = $state<HTMLElement | null>(null);
  const trigger = $derived(wrap?.querySelector("button") ?? null);

  function toggle() {
    open = !open;
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      open = true;
    }
  }
</script>

<span class="menu-button" bind:this={wrap}>
  {#if iconOnly && icon}
    <IconButton
      {label}
      {icon}
      {size}
      variant={variant === "secondary" ? "secondary" : "ghost"}
      {disabled}
      aria-haspopup="menu"
      aria-expanded={open}
      aria-controls={open ? menuId : undefined}
      onclick={toggle}
      {onkeydown}
    />
  {:else}
    <Button
      {variant}
      {size}
      {icon}
      {disabled}
      aria-haspopup="menu"
      aria-expanded={open}
      aria-controls={open ? menuId : undefined}
      onclick={toggle}
      {onkeydown}
    >
      {label}
    </Button>
  {/if}
</span>
<Menu bind:open anchor={trigger} {items} {onselect} {placement} {label} id={menuId} />

<style>
  .menu-button {
    display: inline-flex;
  }
</style>
