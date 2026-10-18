# Silo — Design language & experience

The brief: a tool for tinkerers that advanced simmers respect and newcomers aren't
scared of; not a wall of tiny icons and boxes; a genuinely pleasant place to be;
farm/outdoor color theory carried through every decision. This doc is the source of
truth for look, feel, stack, and language.

## Design pillars

1. **Calm, not crowded.** Generous whitespace, few things per view, one clear
   primary action per screen. Depth comes from *progressive disclosure*, not
   density. A newcomer sees a clean library; a tinkerer opens the drawers.
2. **Earn trust visibly.** Every file-touching action shows what it will do and how
   to undo it. Nothing about the game folder happens invisibly. Trust is essential.
3. **Approachable surface, deep floor.** Plain-language defaults up top; power
   (rules, diffs, graphs, CLI) one layer down. Never make the beginner pay the
   complexity tax, never cap the expert.
4. **Farm-honest aesthetic.** Natural materials and golden-hour light, not neon
   "gamer" chrome and not sterile enterprise gray. It should feel like good tools
   in a clean workshop.
5. **Fast is a feature you can see.** Instant scans, smooth 60fps lists, immediate
   feedback. It should feel solid and immediate.

## Stack decisions (committed)

- **Shell:** Tauri v2 (Rust core) — decided; see `DECISIONS.md` / `ARCHITECTURE.md`.
- **Frontend framework: Svelte 5 (runes) + Vite + TypeScript.** Why: smallest
  runtime and no virtual-DOM overhead (aligns with "native, fast, tiny"); fine-
  grained reactivity avoids whole-store re-render storms; and its low ceremony lets
  us pour effort into *bespoke* visuals rather
  than fighting a component framework. TypeScript throughout for a tinkerer-friendly,
  refactorable codebase.
- **Styling: hand-authored CSS with design tokens** (CSS custom properties), no
  heavyweight UI kit. The farm aesthetic must be ours, not a themed Bootstrap. A
  tiny set of primitives (Button, Card, Field, Toggle, Tag, Toast) built in-house.
- **Icons:** Lucide (`@lucide/svelte`), used **sparingly and always with a label**
  where it matters. Icons accent meaning; they don't replace words. See
  [Icons](#icons).
- **Charts/graphs (later):** lightweight SVG / the `dataviz` approach — for the
  dependency graph, disk-usage, and library-health views.
- **Fonts (self-hosted, offline-bundled):** display/headings **Fraunces** (a warm,
  slightly rustic humanist serif — craft and character); UI/body **Hanken Grotesk**
  (clean, legible, neutral); code and hashes in the system monospace. Numbers
  **tabular** in all data views. See [Typography](#typography).
- **Motion:** gentle, natural easing (wind, not springs). 150–250ms. Respect
  `prefers-reduced-motion`.

## Color system — "Golden Hour on the Farm"

Farm/outdoor color theory: earth, growth, and natural light. Warm neutrals (soil,
linen, oat) instead of cold grays; **field green** as the living primary; **harvest
gold** as the accent that draws the eye; **sky blue** for calm information;
**barn red / terracotta** for danger — warm, never a fire-engine alarm. Two themes
are one idea at two times of day.

Everything lives in `src/lib/styles/tokens.css`. Views use the **semantic slots**
below, never the raw ramps, so both themes come for free.

### Semantic slots (both themes)
| Slot | Light (Daybreak) | Dark (Dusk) | Use |
|---|---|---|---|
| `--bg` | `#F6F3EC` oat | `#171A14` loam | app background |
| `--surface` | `#FCFAF4` linen | `#1F231C` tilled earth | cards, panels |
| `--raised` | `#FFFFFF` | `#282D24` | menus, modals |
| `--sunken` | `#EFEADF` | `#13160F` | wells, inputs, inset areas |
| `--border` | `#E4DECE` | `#39402F` | hairlines, dividers |
| `--border-strong` | `#8F8875` | `#767E66` | control edges (3:1), high-contrast borders |
| `--text` | `#2A2A22` | `#EDEBE0` | primary text |
| `--muted` | `#5E5D50` | `#ADAC9C` | secondary text (6:1 or better) |
| `--primary` | `#4A7330` field | `#7FAE55` pasture | Play, primary buttons, active nav |
| `--primary-hover` | `#395A25` forest | `#9CC06E` | hover/pressed primary |
| `--primary-text` | `#3F6529` | `#9CC06E` | green used as text |
| `--on-primary` | `#FFFFFF` | `#12180E` | text on primary |
| `--accent` | `#E3B23C` wheat | `#E3B23C` wheat | update dots, selection, "new" — a fill, never text in light |
| `--accent-text` | `#8A5F12` | `#F2D26B` | the rare gold-toned text |
| `--on-accent` | `#2A2A22` | `#2A2A22` | text on gold |
| `--info` | `#2F6893` | `#8CBDE0` | neutral info, links, "syncing" |
| `--success` | `#3F6529` | `#9CC06E` | done, verified |
| `--warn-text` | `#94560A` | `#F0B45C` | caution, out-of-date, soft conflict |
| `--danger` | `#A63D2C` | `#EE9481` | destructive, critical conflict |
| `--on-danger` | `#FFFFFF` | `#1A0E0B` | text on danger |
| `--focus` | `#2F5A1D` | `#F2D26B` | the focus ring |
| `--scrim` | loam at 45 % | loam at 60 % | behind modals and drawers |

Soft tints (`--info-soft`, `--success-soft`, `--warn-soft`, `--danger-soft`,
`--accent-soft`) mix 12 % of the slot into `--surface` (24 % under
`prefers-contrast: more`). Earth slots (`--earth`, `--earth-strong`, `--earth-text`,
`--on-earth`) carry soil-brown warmth. The rest of the system is tokens too: spacing
`--sp-1..8` on the 8px grid, radius `--r-sm/md/lg/pill`, elevation `--e1..3`, layers
`--z-*`, motion `--dur-1..3` + `--ease`, and the type scale `--fs-xs..3xl`.

### Brand anchors (the ramps behind the slots)
| Role | Hex | Meaning |
|---|---|---|
| **Field green** | `#5C8A3A` pasture · `#4A7330` field · `#395A25` forest | growth, "go", the brand |
| **Harvest gold** | `#E3B23C` wheat · `#C9922A` amber | the scarce attention color |
| **Sky** | `#4A90C2` | calm information |
| **Squash** | `#D98A1F` | caution |
| **Barn red** | `#B34A38` | danger — warm, never alarm red |
| **Soil brown** | `#8A6A45` | tertiary warmth |

### Rules
- **Never encode meaning by color alone.** Conflict severity, status, and tags
  always pair color with an icon/shape/label (FS players include colorblind users;
  the game ships a colorblind mode — we honor that).
- **WCAG AA everywhere**, in both themes: 4.5:1 body text, 3:1 large text and UI
  edges. Every text/background pairing in the slots above is checked; a new pairing
  gets checked before it ships.
- **Play is field green; gold is the scarce attention color** (update dots,
  selection). Reserve gold for the single thing that most wants attention on a
  screen — overusing it kills its power. **Gold is never text in the light theme**
  (it fails contrast on linen); use `--accent-text` when gold-toned text is needed.
- Dark theme is warm dark (loam), **not** black. Elevation = getting lighter/warmer.

## Typography

- **Fraunces** (variable, with the SOFT axis) for headings and the wordmark;
  **Hanken Grotesk** for all UI and body text. Both ship inside the app as woff2
  with their OFL licences — no network needed, no fallback font in practice.
  Code, hashes and paths use the **system monospace**.
- **rem type scale** `--fs-xs` (12 px) → `--fs-3xl` (36 px) at the default size.
  **12 px is the floor** — nothing smaller, anywhere.
- **Tabular numerals** in data views (sizes, versions, counts) so columns line up.
- **UI size setting** — Settings → Appearance offers 90 / 100 / 110 / 125 %. It
  scales the root size, so text, icons and the silo mark all follow; layouts must
  not clip at 125 %.

## Focus

One visible focus ring everywhere: a 2 px `--focus` outline on `:focus-visible`,
set once globally. Components don't remove it or invent their own. Keyboard users
should always see where they are, in both themes.

## Icons

- **Lucide**, through `src/lib/ui/icons.ts` and `<Icon name="…" />`. Views never
  import Lucide directly.
- **One icon per concept**, the same everywhere. The concepts: library, browse,
  modSets, diagnose, update, tools, settings, help, activity, play, search, problem,
  info, ok, check, verifiedBuild, verifiedAuthor, modified, favorite, hidden,
  visible, flagged, modSettings, showFile, archive, share, download, external, undo,
  rescan, more, controls, bridge, cloud, copy, bug, close, chevrons, tag, grid,
  list, menu, fork, swap, target, arrows, dot, circle, link. A new concept gets a
  new entry; an existing concept never gets a second icon.
- **Sizes:** 16 px in dense UI (rows, chips, inline with text), 20 px in
  navigation. Stroke 1.75. Icons are sized in rem so they follow UI size.
- Icons sit **beside their words**. An **icon-only control needs a label**
  (`aria-label`, plus a tooltip) — no exceptions.

## Silo mark

The mark is a cream grain silo with a domed cap and banded body, a golden-hour sun
rising behind it. Source files live in `src/assets/brand/`; in the app use
`<SiloMark />` (`src/lib/ui/SiloMark.svelte`), which inlines the same drawing.

- **Tile** — the silo on a field-green rounded square. App icon, top bar.
- **Glyph** — silo and sun only, no tile, for light backgrounds and small places.
- **Wordmark** — glyph plus "Silo" in Fraunces 600 (SOFT 50).
- **Clear space** around the mark equals the dome height.
- **Minimum size:** 16 px for the glyph, 24 px for the tile.
- The mark uses fixed brand colors and looks the same in both themes. **The sun
  stays gold**, except in single-color use, where the whole glyph takes one color.

## Layout & density

- **8px spatial grid**; comfortable, roomy. Real padding on cards; no cramped rows.
- Persistent left **nav rail** (few, clear destinations), a calm top bar with the
  one primary action, content in the center with breathing room.
- Mod cards: image-forward, title legible, *at most* 2–3 metadata chips visible;
  everything else on the detail view. No card should look like a spreadsheet cell.
- Empty states are a feature: warm, illustrated-ish, they teach the next step in
  plain, translatable language.

## Language & terminology

Follow the GIANTS in-game approach: **plain, literal words that translate cleanly
into dozens of languages.** No invented vocabulary, no metaphor decoder ring. Use
the obvious terms — *library, loadout, apply, clear, conflict, health check,
updates, savegame mods, favorite, hidden, broken.* "Silo" is the app's working
name only; it is not a term users have to learn.

Terminology is deliberately **not finalized here** — final wording (and any light
flavor) will be refined by hand once the app is built and every string is a
translation key. Until then, code and UI use plain descriptive names.

## Give the tinkerers things to do (power layer)

The floor under the calm surface — this is what makes advanced simmers adopt it and
evangelize it:

- **XML settings, rendered as a real form.** Most mods expose tunable options only
  as hand-edited XML. In each mod's tinkerer area, Silo reads the mod's settings
  XML and **generates a clean, stylized, editable form** — toggles, sliders, number
  fields, dropdowns — inferred from the values (and the XSD where one exists), then
  writes changes back to the XML safely. See `ARCHITECTURE.md#settings-form-generator`.
  This is the flagship tinkerer feature: it turns "open the file in Notepad" into a
  first-class UI.
- **Loadouts you can compose & compare.** Build, clone, and **diff two loadouts**
  (what mods/versions differ). Import/export a loadout as a shareable file.
- **Conflict explorer.** Every conflict drills down to the exact `modDesc.xml`
  element and the two mods involved (shared specialization/type/uniqueType/script),
  with a plain-English "why this clashes."
- **Health-check rules engine.** User-defined rules over the library: e.g. *tag any
  mod >200 MB "heavy," flag mods sharing a `<uniqueType>`, warn on `extraSourceFiles`
  overlap, surface digit-prefixed names.* Ship a strong default rule-pack; let
  power users add their own.
- **Dependency graph.** Visualize the active loadout's dependency web; highlight
  missing/optional deps and the `required="true"` mods a chosen save needs.
- **Snapshots & rollback.** Snapshot the mods-folder state before any change; one-
  click restore. (Reversibility made tangible.)
- **Savegame binding.** Generate a loadout from a save's mod list; verify a save's
  `required` mods are present and version-matched (via `fileHash`).
- **Saved filters / queries** over the library (by author, category, size, age,
  MP-support, "has scripts," "touches shop"). Bulk actions on the result set.
- **CLI / headless mode.** Apply a loadout, run a health check, export a report from
  a script — for the automation-minded. (The Rust core makes this cheap: the GUI and
  CLI call the same engine.)
- **Per-mod notes, personal rating, and a changelog trail.** The curation memory
  the community lacks today.

## What "doing it right" looks like at a glance

In the first ten seconds, it should read as: it opened
instantly; the library is calm and beautiful, not a grid of tiny boxes; it clearly
knows my mods (icons, versions, conflicts) without me configuring anything; and the
one big field-green Play button tells me exactly what to do next. Fast, honest, handsome.
