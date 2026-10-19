# Base Save generator — scope

> **Status: not built.** A post-1.0 idea; nothing here ships in Silo 1.0.

Create ready-to-play FS25 savegames **outside the game**, by cloning a real base save
and rewriting the parameters we understand. Not by mimicking the engine's init — that's
off the table (see "Why not from scratch").

## Why not from scratch

A savegame folder isn't just XML. Alongside `careerSavegame.xml` it carries **binary
map layers** — `densityMap_*.gdm`, `infoLayer_*.grle`, `precisionFarming_*.grle` (field
state, ground type, growth, weeds, plow/lime/spray levels). The engine generates these
from the map on "New Game." Reproducing them (and the exact `careerSavegame revision`
format) outside the engine means reimplementing GIANTS' map init — stripped in
gameSource, version-fragile. So we **start from an engine-produced base save** (valid by
construction) and only edit what we understand. This is how the community's "Increased
Income Savegame" mods already work; we generalize + automate it.

## The load-time vs state distinction (the crux)

Not every `careerSavegame.xml <settings>` value is safe to edit post-hoc. Two classes:

- **Load-time settings** — re-read by the engine every load. Editing them in
  `careerSavegame.xml` *takes effect*. These are the gameplay toggles: `growthMode`,
  `plannedDaysPerPeriod`, `weedsEnabled`, `stonesEnabled`, `limeRequired`,
  `plowingRequiredEnabled`, `isSnowEnabled`, `fuelUsage`, `helperBuy*`, `trafficEnabled`,
  `stopAndGoBraking`, `savegameName`, etc.
- **State** — baked into other files when the save was created; editing
  `careerSavegame.xml` alone does nothing. Money lives in `farms.xml` (and is echoed in
  `careerSavegame.initialMoney` for display); owned farmland lives in `farmland.xml`'s
  ownership grid; the farm name lives in `farms.xml`.

So a correct edit touches the **right file**: money → `farms.xml` (+ mirror
`initialMoney`), farm name → `farms.xml` (+ `savegameName`), gameplay → `careerSavegame.xml`.

## v1 scope (safe, high-value)

A "New base save" flow:

1. **Pick a template** (a captured fresh base save for a map — see Templates).
2. **Set parameters:**
   - Savegame name → `careerSavegame.savegameName`.
   - Farm name → `farms.xml farm@name`.
   - **Starting money** → `farms.xml farm@money` (+ mirror `careerSavegame.initialMoney`).
     This is the popular use case ("increased income" saves), done right.
   - Gameplay/growth toggles → `careerSavegame.xml` (the load-time list above).
   - Economic difficulty → `careerSavegame.economicDifficulty` (affects ongoing prices;
     note some of its effect is baked, so label it "best-effort").
3. **Generate** → clone the template folder into the next free `savegameN/` slot,
   rewrite the XML (quick-xml, which Silo already uses), leave every binary layer as-is.

**Deferred to later:** owned-farmland toggle (needs `farmland.xml` grid edits — ship as
template variants "owned"/"not owned" instead), starting vehicles/placeables, mission
state.

## Templates

A template = a base-save folder + a small manifest (`mapId`, `mapTitle`, source, game
version it was captured on, which params are editable). Seed sources:

- **Capture from the user's own fresh save** — "Save this as a base template" on a
  freshly-created game. Zero legal issue (their save).
- **Curated built-ins** for base-game maps — we generate these once and ship them.
- **Import/export** a template file so people can share base saves (a natural community
  artifact, and a Silo hook).

## Constraints & guardrails

- **Map dependency:** the template's map must be installed or the save won't load.
  Silo already knows the library — check and warn (base-game maps always OK).
- **Version drift:** store the captured game version in the manifest; warn on mismatch.
  Cloning a real save means minor game updates usually migrate cleanly; we never claim
  byte-perfect parity.
- **Never clobber:** write into the next *free* `savegameN/` slot; never overwrite an
  existing save. Reversible by construction (it's a new folder).
- **Validate the edit actually takes effect** before trusting it — see Phase 0.

## Build phases

- **Phase 0 — spike (validation-first).** Clone one base-game template, set money via
  `farms.xml`, generate a `savegameN/`, load it in-game, confirm the money is right and
  the save is stable. Confirms the load-time/state model before any UI. *This is the
  go/no-go.*
- **Phase 1 — the flow.** Template capture + the generate form (name, farm, money,
  gameplay toggles) + free-slot management + map-installed check. Rust does the
  clone+XML-rewrite off-thread; a thin panel drives it.
- **Phase 2 — templates as content.** Curated built-ins for base-game maps;
  export/import template files for sharing.
- **Phase 3 — advanced.** Owned-farmland variants, starting equipment, more settings.

## Where it lives

Rust module `basesave.rs` (pure: clone dir + parametrize XML, unit-testable against a
fixture save), thin Tauri commands, a `BaseSaveStudio.svelte` panel. Reuses the existing
savegame discovery + quick-xml. Fits Silo's "reversible, off-thread, parse-don't-regex"
principles cleanly.
