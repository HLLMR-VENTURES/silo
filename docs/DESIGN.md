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
| `--border-strong` | `#8A8370` | `#767E66` | ≥3:1 control edges on every surface, `--sunken` included; the border under high contrast |
| `--text` | `#2A2A22` | `#EDEBE0` | primary text |
| `--muted` | `#5E5D50` | `#ADAC9C` | secondary text, AA (5.5:1 or better on every surface) |
| `--primary` | `#4A7330` field | `#7FAE55` (dark-theme primary) | Play, primary buttons, active nav |
| `--primary-hover` | `#395A25` forest | `#9CC06E` | hover/pressed primary |
| `--primary-text` | `#3F6529` | `#9CC06E` | green used as text |
| `--on-primary` | `#FFFFFF` | `#12180E` | text on primary |
| `--accent` | `#E3B23C` wheat | `#E3B23C` wheat | update dots, selection, "new" — a fill, never text in light |
| `--accent-text` | `#8A5F12` | `#F2D26B` | the rare gold-toned text |
| `--on-accent` | `#2A2A22` | `#2A2A22` | text on gold |
| `--info` | `#2D6490` | `#8CBDE0` | neutral info, links, "syncing" |
| `--success` | `#3F6529` | `#9CC06E` | done, verified |
| `--warn-text` | `#905309` | `#F0B45C` | caution, out-of-date, soft conflict |
| `--danger` | `#A63D2C` | `#EE9481` | destructive, critical conflict |
| `--on-danger` | `#FFFFFF` | `#1A0E0B` | text on danger |
| `--focus` | `#2F5A1D` | `#F2D26B` | the focus ring |
| `--scrim` | loam at 45 % | loam at 60 % | behind modals and drawers |

Soft tints (`--info-soft`, `--success-soft`, `--warn-soft`, `--danger-soft`,
`--accent-soft`) mix 12 % of the slot into `--surface`. High-contrast mode
(`prefers-contrast: more`) strengthens borders: `--border` becomes
`--border-strong`. Earth slots (`--earth`, `--earth-strong`, `--earth-text`,
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
- **UI size setting** — Settings → Appearance offers 90% / 100% / 110% / 125%. It
  scales the root size, so text, icons and the silo mark all follow; layouts must
  not clip at 125%.

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
  list, menu, fork, swap, target, arrows, dot, circle, link, map, script, folder.
  A new concept gets a new entry; an existing concept never gets a second icon.
- **Sizes:** 13–14 px inline with text, 16 px in icon buttons, 18–20 px in the
  top bar. Stroke 1.75. Icons are sized in rem so they follow UI size.
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

## Components

Views build from the primitives in `src/lib/ui/` (import them from the `index.ts` barrel)
instead of styling raw elements. Each one carries the focus ring, the color slots and the
keyboard behavior, so a view gets them for free. Each file's header comment shows its usage.

**Controls**
- **Button**: text button (primary / secondary / ghost / danger / link), optional icon; `loading`
  shows a spinner, keeps the width and ignores clicks without dropping focus.
- **IconButton**: icon-only button; its `label` is required and becomes the name and tooltip.
- **Switch**: on/off for a setting that applies at once.
- **Checkbox**: a choice inside a form or a selection (supports indeterminate).
- **RadioGroup**: one of a few options, shown as native radios in a fieldset.
- **Segmented**: one of two to four short options shown side by side (view, sort order).
- **Select**: styled native dropdown for longer option lists.
- **TextField**: single-line text input.
- **Field**: label + control + hint + error, wired up with ids and `aria-describedby`.

**Display**
- **Badge**: Silo's verdict on a mod (Verified, Update, Broken) in a soft pill. Rows of
  badges go through `capBadges()`: at most three, then "+n".
- **Chip**: an active filter, with a remove button.
- **Tag**: the user's own tag on a mod; neutral on purpose, because tags are their words.
- **ProgressBar**: progress with visible text; indeterminate when there's no value.
- **Skeleton**: loading placeholder; hidden from screen readers; the shimmer stops under reduced motion.
- **EmptyState**: nothing here yet, with one sentence on what to do next.
- **ErrorState**: something failed, with "Copy details" when there are details.
- **Tabs**: tabs with one panel, roving focus with the arrow keys.
- **Kbd**: a keyboard key.

**Overlays**
- **Dialog**: modal; everything behind it is inert. The base for the two below.
- **ConfirmDialog**: yes/no with a verb button, optional facts table and affected-items list.
  Reached through `ask()`.
- **PromptDialog**: one text field with inline validation. Reached through `askText()`.
- **Drawer**: non-modal side panel (mod details); the page behind stays usable; resizable.
- **Popover**: floating panel anchored to an element or a point; flips to stay on screen.
- **Menu** / **MenuButton**: action menu on a Popover, and a trigger button paired with one.
- **Tooltip** / `use:tooltip`: a short hint on hover or keyboard focus. Never the only place for
  information the user needs.
- **Toast** / **ToastRegion**: short notices from `toast()`, optionally with one action (Undo).

`DialogHost` and `ToastRegion` are mounted once, in `App.svelte`. Component tests
(vitest + jsdom, `npm test`) live beside the primitives in `src/lib/ui/`.

### Confirmations

- **Always `await ask({...})`** (or `await askText({...})` for a name). An un-awaited call is a
  Promise, which is always truthy, so a `!ask(...)` guard would never stop the action. The
  build guard `scripts/check-confirm.mjs` fails on un-awaited `ask` / `askText` / `askOrOs` /
  `confirmAction` / `confirmDialog` calls (member calls like `ui.ask(` included; the body of an
  arrow counts only when the arrow is `async`), on importing one of them under another name, and
  on any `confirm()`, `window.confirm`, `prompt()` or `window.prompt`.
- **The button says the verb**, never "OK": "Delete", "Switch", "Move library", "Replace".
  Cancel stays "Cancel" unless a clearer pair exists ("Keep going" / "Cancel move").
- **The title is the action as a question** ("Delete Harvest 2026?", "Switch to Modded
  farm?"). **The body says the consequence, then how to undo it** ("Your mods aren't
  affected." / "To undo it, run Organize again."). If there is no way back, say so.
- **Show what changes.** Counts go in the facts table and affected mods in the list, so the
  user sees exactly what the button will do (switching a loadout lists the mods that turn on
  and the mods that get parked).
- **Danger tone only when Silo can't undo it** (deleting a collection from GitHub, forgetting
  an interrupted diagnosis) or when the Undo is only a short-lived notice (deleting a
  loadout). Danger makes the button red, puts the starting focus on Cancel and makes the dialog
  an `alertdialog`. The body is the dialog's accessible description.
- **Offer Undo after the fact** where Silo can put things back: a toast with an Undo action
  after switching a loadout, using a savegame's mods, replacing a loadout's mods or deleting a
  loadout. An Undo checks first that nothing moved on (the active set is still the one the
  switch left, no diagnosis or library move is running, the loadout wasn't changed again) and
  otherwise says why it can't undo instead of guessing. A new switch dismisses the previous
  switch's Undo.
- **One gesture answers one dialog.** A confirm's verb button (and a name dialog's submit)
  ignores clicks and Enter/Space for `ARM_MS` (300 ms) after the dialog opens, and again after
  a name dialog's button turns into "Replace", and never reacts to a held, repeating key. So a
  double-click or a long Enter meant for one dialog can't answer the next one in the queue.
  Cancel, Esc and the scrim work at once.
- **The OS dialog is only for two cases**: the quit guard (it has to work whatever state the
  window is in) and code that can run before the app shell mounts. The latter uses
  `askOrOs()` from `src/lib/confirm.ts`, which asks in-app once the dialog host is up and
  falls back to the OS dialog with the same text before that.

### Focus in dialogs, menus and toasts

- **Dialogs** trap Tab and make the page behind inert. Starting focus: Cancel for a danger
  confirm, the verb button for any other confirm, the text field for a name dialog (its text
  selected). Esc and the scrim cancel; the action does not run. On close, focus returns to
  whatever had it before the dialog opened. Dialogs queue: one is on screen at a time.
- **Esc goes to the top layer only.** Dialogs, popovers and menus share one overlay stack, so
  Esc in a menu inside a dialog closes the menu, not the dialog.
- **Menus** start on the first enabled item. Up/Down wrap, Home/End jump, typing a letter
  jumps to the matching item, Enter/Space pick, Esc closes and returns focus to the trigger,
  and Tab closes.
- **Popovers** close on a click outside, on Esc (focus returns to the anchor) and when focus
  tabs out of them.
- **Drawers** move focus to their title when they open and return it on close, but don't trap
  it; the page behind stays usable.
- **Toasts never take focus.** They're announced through a live region that stays readable
  while a dialog is open; their buttons are inert until it closes. Tab can reach their action
  button, whose name says what it acts on ("Undo: Switched to Harvest."). The countdown pauses while the
  toast is hovered or focused and while Silo's window is in the background, so alt-tabbing
  away doesn't make an Undo disappear. A toast with an action stays 10 s, others 6 s.

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
