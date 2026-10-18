<script lang="ts">
  // Test-only: one UI primitive, rendered with typical props, for the axe suite (#258).
  import {
    Badge,
    Button,
    Checkbox,
    Chip,
    Dialog,
    Drawer,
    EmptyState,
    ErrorState,
    Field,
    Icon,
    IconButton,
    Kbd,
    Menu,
    MenuButton,
    Popover,
    ProgressBar,
    RadioGroup,
    Segmented,
    Select,
    SiloMark,
    Skeleton,
    Switch,
    Tabs,
    Tag,
    TextField,
    Tooltip,
  } from "../../ui";

  let { which }: { which: string } = $props();

  let anchor = $state<HTMLElement | null>(null);
  let text = $state("");
  const choices = [
    { value: "list", label: "List", icon: "list" as const },
    { value: "grid", label: "Grid", icon: "grid" as const },
  ];
  const menuItems = [
    { id: "open", label: "Open folder", icon: "folder" as const },
    { id: "rename", label: "Rename", shortcut: "F2" },
    { id: "delete", label: "Delete", danger: true, separatorBefore: true },
  ];
</script>

{#if which === "badge"}
  <Badge tone="success" icon="verifiedBuild">Verified build</Badge>
  <Badge tone="warn" icon="problem">Modified</Badge>
  <Badge tone="danger" icon="problem">Broken</Badge>
  <Badge>Map</Badge>
{:else if which === "button"}
  <Button variant="primary" icon="play">Play</Button>
  <Button>Cancel</Button>
  <Button variant="danger">Delete</Button>
  <Button variant="ghost" loading>Saving</Button>
  <Button variant="link" href="https://example.invalid">Read more</Button>
  <Button disabled>Disabled</Button>
{:else if which === "iconButton"}
  <IconButton icon="close" label="Close" />
  <IconButton icon="favorite" label="Favorite" pressed={true} />
  <IconButton icon="rescan" label="Rescan" disabled />
{:else if which === "switch"}
  <Switch label="Show hidden mods" />
  <Switch ariaLabel="Active: Courseplay" checked />
{:else if which === "checkbox"}
  <Checkbox label="Select all" indeterminate />
  <Checkbox label="Also delete the file" checked />
{:else if which === "chip"}
  <Chip label="Category: Tractors" onremove={() => {}} />
  <Chip label="Active" icon="ok" />
{:else if which === "tag"}
  <Tag>harvest</Tag>
  <Tag showIcon>forestry</Tag>
{:else if which === "kbd"}
  <p><Kbd>Ctrl</Kbd>+<Kbd>K</Kbd> focuses search.</p>
{:else if which === "icon"}
  <p><Icon name="info" /> Decorative beside text.</p>
  <Icon name="problem" label="Problem" />
{:else if which === "field"}
  <Field label="Set name" hint="Shown in the Play menu." required>
    {#snippet control(c)}
      <TextField id={c.id} aria-describedby={c.describedBy} invalid={c.invalid} bind:value={text} required />
    {/snippet}
  </Field>
  <Field label="Folder" error="That folder doesn't exist.">
    {#snippet control(c)}
      <TextField id={c.id} aria-describedby={c.describedBy} invalid={c.invalid} value="Z:/nope" />
    {/snippet}
  </Field>
  <Field label="Density">
    {#snippet control(c)}
      <Select id={c.id} aria-describedby={c.describedBy} options={choices} value="list" />
    {/snippet}
  </Field>
{:else if which === "radio"}
  <RadioGroup legend="Theme" options={[{ value: "light", label: "Light", hint: "Daybreak" }, { value: "dark", label: "Dark" }]} value="light" />
  <RadioGroup legend="Hidden legend" hideLegend options={choices} value="grid" />
{:else if which === "segmented"}
  <Segmented ariaLabel="View" options={choices} value="list" />
{:else if which === "tabs"}
  <Tabs tabs={[{ id: "a", label: "My sets" }, { id: "b", label: "Savegames", badge: 3 }]} ariaLabel="Mod sets">
    {#snippet panel(id)}<p>Panel {id}</p>{/snippet}
  </Tabs>
{:else if which === "progress"}
  <ProgressBar label="Copying 3 of 10" value={3} max={10} />
  <ProgressBar label="Working" hideLabel />
{:else if which === "skeleton"}
  <Skeleton />
  <Skeleton variant="circle" />
{:else if which === "empty"}
  <EmptyState icon="library" title="No mods yet" body="Point Silo at your mods folder." level={2}>
    {#snippet actions()}<Button variant="primary">Choose folder</Button>{/snippet}
  </EmptyState>
{:else if which === "error"}
  <ErrorState title="Couldn't reach the catalog" body="Check your connection." details="ECONNREFUSED" level={2}>
    {#snippet actions()}<Button>Try again</Button>{/snippet}
  </ErrorState>
{:else if which === "mark"}
  <SiloMark variant="tile" />
  <SiloMark variant="wordmark" label="Silo" />
  <span><SiloMark variant="glyph" label={false} /> Silo</span>
{:else if which === "tooltip"}
  <Tooltip text="Rescan library">
    {#snippet children()}<IconButton icon="rescan" label="Rescan" />{/snippet}
  </Tooltip>
{:else if which === "menuButton"}
  <MenuButton label="More library actions" icon="more" iconOnly items={menuItems} onselect={() => {}} />
  <MenuButton label="Sort" items={menuItems} onselect={() => {}} />
{:else if which === "menu"}
  <button type="button" bind:this={anchor}>Anchor</button>
  <Menu open {anchor} items={menuItems} label="Mod actions" onselect={() => {}} />
{:else if which === "popover"}
  <button type="button" bind:this={anchor} aria-expanded="true" aria-controls="pop">Filters</button>
  <Popover open {anchor} id="pop" aria-label="Filters">
    <Checkbox label="Favorites only" />
  </Popover>
{:else if which === "dialog"}
  <Dialog open title="Rename set" description="Pick a new name.">
    <Field label="Name">
      {#snippet control(c)}<TextField id={c.id} value="Harvest" />{/snippet}
    </Field>
    {#snippet footer()}<Button>Cancel</Button><Button variant="primary">Rename</Button>{/snippet}
  </Dialog>
{:else if which === "drawer"}
  <Drawer open title="Courseplay">
    <p>Details</p>
  </Drawer>
{/if}
