<script lang="ts">
  // Settings → General: theme, UI size, the Library's list density, checking for mod updates at
  // startup (off unless turned on; persisted by updates/autoCheck), and a pointer to the game's
  // graphics settings.
  //   <GeneralSettings {theme} {uiScale} {density} {userDir} {onSetTheme} {onSetUiScale}
  //     {onSetDensity} {onOpenGameGraphics} />
  import Button from "../../ui/Button.svelte";
  import Segmented from "../../ui/Segmented.svelte";
  import Select from "../../ui/Select.svelte";
  import Switch from "../../ui/Switch.svelte";
  import type { ChoiceOption } from "../../ui/types";
  import { UI_SCALES, parseUiScale, type UiScale } from "../../uiScale";
  import type { Density } from "../library/ModListRow.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import { autoCheck, setAutoCheck, UPDATE_PRIVACY_LINE } from "../../updates/autoCheck.svelte";

  type Theme = "system" | "light" | "dark";

  interface Props {
    theme: Theme;
    uiScale: UiScale;
    /** FS25's user folder; the graphics pointer shows only when it's known. */
    userDir: string | null;
    onSetTheme: (t: Theme) => void;
    onSetUiScale: (s: UiScale) => void;
    /** The Library list's row height. */
    density?: Density;
    onSetDensity?: (d: Density) => void;
    /** Opens Tools → Graphics settings (the game.xml editor lives there). */
    onOpenGameGraphics: () => void;
  }

  let {
    theme,
    uiScale,
    userDir,
    onSetTheme,
    onSetUiScale,
    density = "comfortable",
    onSetDensity,
    onOpenGameGraphics,
  }: Props = $props();

  const THEMES: ChoiceOption<Theme>[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];
  const DENSITIES: ChoiceOption<Density>[] = [
    { value: "comfortable", label: "Comfortable" },
    { value: "compact", label: "Compact" },
  ];
  const SCALES: ChoiceOption[] = UI_SCALES.map((s) => ({
    value: String(s),
    label: `${Math.round(s * 100)}%`,
  }));
</script>

<SettingsCard title="Appearance">
  <div class="row">
    <span class="label">Theme</span>
    <Segmented
      ariaLabel="Theme"
      options={THEMES}
      value={theme}
      onchange={(t) => onSetTheme(t)}
    />
  </div>
  <p class="hint">System follows your computer's light or dark setting.</p>
  <div class="row">
    <label class="label" for="set-ui-size">UI size</label>
    <Select
      id="set-ui-size"
      options={SCALES}
      value={String(uiScale)}
      onchange={(e) => onSetUiScale(parseUiScale(e.currentTarget.value))}
    />
  </div>
  <p class="hint">Makes text and icons smaller or larger everywhere in Silo.</p>
  <div class="row">
    <span class="label">List density</span>
    <Segmented
      ariaLabel="List density"
      options={DENSITIES}
      value={density}
      onchange={(d) => onSetDensity?.(d)}
    />
  </div>
  <p class="hint">Compact fits more mods on screen in the Library's list.</p>
</SettingsCard>

<SettingsCard title="Updates">
  <div class="row">
    <span class="label">Check for mod updates when Silo starts</span>
    <Switch
      checked={autoCheck.on}
      controlled
      ariaLabel="Check for mod updates when Silo starts"
      onchange={(on) => setAutoCheck(on)}
    />
  </div>
  <p class="hint">
    {UPDATE_PRIVACY_LINE} Silo only checks. You choose what to install in Updates.
  </p>
</SettingsCard>

{#if userDir}
  <SettingsCard
    title="Graphics settings"
    description="Tune Farming Simulator 25's graphics (presets: Performance / Balanced / Quality) without launching the game. Also in Tools → Graphics settings."
  >
    {#snippet actions()}
      <Button icon="modSettings" onclick={onOpenGameGraphics}>Edit graphics settings</Button>
    {/snippet}
  </SettingsCard>
{/if}

