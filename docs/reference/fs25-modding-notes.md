# FS25 modding notes (for Silo's engine)

Ground-truth facts about how FS25 structures and loads mods. **Authoritative**
where noted: derived directly from the game's own schema
(`shared/xml/schema/modDesc.xsd`, `descVersion` 105-era) and real files on disk.
A local (gitignored) mirror of the schema + samples lives in `../../reference/`.

## File locations

### Windows
- **User dir:** `Documents/My Games/FarmingSimulator2025/`
  - `mods/` — the flat folder FS25 reads. A mod = `Foo.zip` **or** an unpacked
    `Foo/` dir containing `modDesc.xml`.
  - `gameSettings.xml` — `<modsDirectoryOverride active="true|false" directory="…"/>`
    points the game at a custom mods folder. Also `<showAllMods>` toggles the
    mod-selection filter. (Authoritative schema: `gameSettings.xsd`.)
  - `savegame1/`…`savegameN/` — each has `careerSavegame.xml` (+ farms/vehicles/
    placeables/etc.). The save's mod list lives in `careerSavegame.xml` (below).
  - `modSettings/<modName>/` — per-mod settings the game persists.
  - `pdlc/` — paid DLC (`pdlc_*`), tracked in saves like mods but not user mods.
  - **`modManagerTemplates/` and `modManagerArchives/` are another mod manager's
    folders**, not the game's. Silo must NOT reuse those names — pick our
    own (e.g. a single app-data dir under the OS-appropriate location).
- **Game install (Steam):** e.g. `E:\SteamLibrary\steamapps\common\Farming Simulator 25`
  - `sdk/debugger/scriptBinding.xml`, `sdk/debugger/gameSource.zip` — engine + game
    Lua reference.
  - `shared/xml/schema/*.xsd` — **88 authoritative schemas** incl. `modDesc.xsd`,
    `gameSettings.xsd`, and `savegame_*.xsd`.
  - `pdlc/` — installed DLC.

### Cross-platform — see [../CROSS-PLATFORM.md](../CROSS-PLATFORM.md) for Mac/Linux
paths, Steam-library discovery, and the projection strategy per OS.

## How FS25 decides what to load

- It scans **only the flat `mods/` root** — no subfolder recursion. This is why
  "organized" today means "one giant flat folder," and why Silo projects a chosen
  set in at launch.
- **A mod whose name starts with a digit is ignored by the engine.** `07_Foo.zip`
  silently won't load — Silo flags this as a health issue.
- `.zip` and unpacked-dir forms are both valid; identity is the **tech name** =
  the zip/dir basename (e.g. `FS25_AdjustEnginePower`).

## `careerSavegame.xml` — the save's mod list (authoritative, from real saves)

```xml
<mod modName="FS25_precisionFarming" title="Precision Farming"
     version="1.5.0.0" required="false"
     fileHash="29fd0411b119c19238541475fa86dea0"/>
```

- `modName` — tech name (matches the folder/zip basename; `pdlc_*` for DLC).
- `version` — the version the save was last played with. Compare to library.
- **`required`** — `true` ⇒ the save genuinely needs this mod. **A profile derived
  from a save MUST include every `required="true"` mod**; missing one blocks/breaks
  the save. `required="false"` ⇒ present-but-optional (safe to omit).
- `fileHash` — **MD5** of the mod file. Silo can verify a library mod matches what
  the save expects (mismatch ⇒ "different version than the save was built on").

## `modDesc.xml` — AUTHORITATIVE field reference (from `modDesc.xsd`)

Root: `<modDesc descVersion="INT">` — `descVersion` **required** (105 in current
FS25; gates the min game patch). Parse with `quick-xml`, never regex.

**Required children:** `<author>` (string), `<version>` (string, `a.b.c.d`),
`<iconFilename>` (path to mod icon, usually `.dds`, sometimes `.png`).

**Core metadata:**
- `<title>` — localized: child tags per language (`en de fr pl ru … 26 langs`).
  Prefer `en`, fall back to any, then the tech name. May contain CDATA.
- `<description>` — localized the same way; CDATA common.
- `<multiplayer supported="bool" only="bool"/>` — MP support/only flags → a
  "MP-safe" filter for free.
- `<isSelectable>` (bool) — shows in the mod-selection screen.
- `<uniqueType>` (string) — **GIANTS' own conflict primitive:** "only one mod of
  this type may be selected." Two active mods sharing a `<uniqueType>` conflict by
  design. Treat as a **critical** conflict signal.

**Dependencies (authoritative):** `<dependencies><dependency>` where each
`<dependency>` is a **string** = "filename of the mod (without `.zip`) to be
installed for this mod to be used." So a dependency is a **tech name**, not a
struct. (Some community mods non-canonically append a URL in the text/attrs — parse
defensively: take the tech-name token, optionally recover a `mod_id` from any URL,
but never assume a shape. A blank library must not crash — parse defensively.)

**Namespace-collision surfaces (the raw material for conflict detection).** Each is
a place two mods can register the same name and clash:

| Element | Child → key | Collision severity |
|---|---|---|
| `<specializations>` | `<specialization name … className filename>` | none: namespaced per mod (see below) |
| `<placeableSpecializations>` | `<specialization name …>` | none: namespaced per mod (see below) |
| `<handToolSpecializations>` | `<specialization name …>` | none: namespaced per mod (see below) |
| `<vehicleTypes>` | `<type name … className filename parent>` | none: namespaced per mod (see below) |
| `<placeableTypes>` | `<type name …>` | none: namespaced per mod (see below) |
| `<handToolTypes>` | `<type name …>` | none: namespaced per mod (see below) |
| `<extraSourceFiles>` | `<sourceFile filename>` (global Lua) | warning + safety flag |
| `<actions>` | `<action name …>` | warning (input action clash) |
| `<inputBinding>` | `<actionBinding action><binding …>` | info/warning |
| `<brands>` | `<brand name title image>` | warning |
| `<storeCategories>` | `<storeCategory name title type>` | warning |
| `<uniqueType>` | (whole-mod) | critical (by GIANTS design) |

**Types and specializations are namespaced per mod.** `TypeManager.lua` (gameSource) registers
a mod's `<type name="x">` as `<modName>.x` and looks up a type's `<specialization name="y">`
first as `y`, then as `<modName>.y`, which only works because mod specializations are
registered under the mod's name too. So two mods using the same type or specialization name
both load; Silo doesn't flag them.

`<extraSourceFiles>` doubles as a **safety** signal: it injects Lua into the global
game state. High source-file counts / overlap with other mods = elevated risk.

**Content/inventory (for library richness, not conflicts):**
- `<storeItems><storeItem xmlFilename>` — count = # shop items the mod adds.
- `<maps><map id className configFilename filename>` with localized `<title>` and
  `<iconFilename>`. **Presence of `<maps>` ⇒ map mod.** Maps are huge (100 MB+);
  stream the zip central directory, don't load the whole archive for the icon.
- `<l10n>` (`<text name>` + langs, or external `filenamePrefix`), `<fillTypes>`,
  `<fruitTypes>`(?), `<brands>`, `<materialHolders>`, `<materialTemplates>`,
  `<bales>`, `<wildlife>`, `<connectionHoses>`, `<jointTypes>`, `<missionVehicles>`.
- `<parentFile xmlFilename>` with `<set/remove/clearList>` — mod patches another
  XML; a signal the mod extends/overrides base content.

## Projection mechanics — see [../ARCHITECTURE.md](../ARCHITECTURE.md#projection)
Symlink/junction vs copy strategy differs per OS; captured in ARCHITECTURE +
CROSS-PLATFORM.

## Reference corpus & fixtures (local, gitignored)
- `Documents/My Games/FarmingSimulator2025/mods/` — **729 real mods** here. Primary
  fixture for scan performance and conflict accuracy.
- `reference/schema/modDesc.xsd`, `gameSettings.xsd` — mirrored authoritative schemas.
- `reference/samples/sample_modDesc_script.xml` — a real script mod (global Lua
  injection, MP-supported, localized) for parser unit tests.
- Deliberately collect fixtures covering: a large map mod, a mod with
  `<specializations>`, a mod with a URL-form dependency (the crash case), a
  digit-prefixed name, a corrupt zip.

## Still to verify against SDK
- [ ] `<fruitTypes>` element name (not seen in modDesc.xsd this pass — may live in a
      map's config, not modDesc).
- [ ] Whether FS25 dedupes by tech name when both `Foo.zip` and `Foo/` exist.
- [ ] `pdlc_*` discovery on Mac/Epic layouts.
