<script lang="ts">
  // Settings → General: theme, UI size, and a pointer to the game's graphics settings.
  //   <GeneralSettings {theme} {uiScale} {userDir} {onSetTheme} {onSetUiScale} {onOpenGameGraphics} />
  import Button from "../../ui/Button.svelte";
  import Segmented from "../../ui/Segmented.svelte";
  import Select from "../../ui/Select.svelte";
  import type { ChoiceOption } from "../../ui/types";
  import { UI_SCALES, parseUiScale, type UiScale } from "../../uiScale";
  import SettingsCard from "./SettingsCard.svelte";

  type Theme = "system" | "light" | "dark";

  interface Props {
    theme: Theme;
    uiScale: UiScale;
    /** FS25's user folder; the graphics pointer shows only when it's known. */
    userDir: string | null;
    onSetTheme: (t: Theme) => void;
    onSetUiScale: (s: UiScale) => void;
    /** Opens Tools → Graphics settings (the game.xml editor lives there). */
    onOpenGameGraphics: () => void;
  }

  let { theme, uiScale, userDir, onSetTheme, onSetUiScale, onOpenGameGraphics }: Props = $props();

  const THEMES: ChoiceOption<Theme>[] = [
    { value: "system", label: "System" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
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
</SettingsCard>

{#if userDir}
  <SettingsCard
    title="Graphics settings"
    description="Tune Farming Simulator 25's graphics (presets: Performance / Balanced / Quality) without launching the game. Also in Tools → Graphics settings."
  >
    {#snippet actions()}
      <Button icon="modSettings" onclick={onOpenGameGraphics}>Edit game.xml</Button>
    {/snippet}
  </SettingsCard>
{/if}

