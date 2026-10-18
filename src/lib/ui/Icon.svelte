<script lang="ts">
  // Single icon renderer: <Icon name="close" />. Decorative (aria-hidden) unless `label` is set.
  // `size` is in px at the default UI size; it renders in rem so icons follow the UI size setting.
  import { ICONS, type IconName } from "./icons";

  interface Props {
    name: IconName;
    size?: number;
    /** Accessible name; omit for decorative icons beside visible text. */
    label?: string;
    /** Fill the shape with currentColor (e.g. a filled favorite star). */
    filled?: boolean;
    class?: string;
  }

  let { name, size = 16, label, filled = false, class: className = "" }: Props = $props();

  const Cmp = $derived(ICONS[name]);
  const rem = $derived(`${size / 16}rem`);
</script>

<Cmp
  {size}
  strokeWidth={1.75}
  fill={filled ? "currentColor" : "none"}
  class={className}
  style="width: {rem}; height: {rem}"
  aria-hidden={label ? undefined : "true"}
  role={label ? "img" : undefined}
  aria-label={label}
  focusable="false"
/>
