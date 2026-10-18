# Silo — Smoke-Test Checklist (release gate)

> **For a release candidate, read [`RC_TESTING.md`](RC_TESTING.md) FIRST.** It's the hostile-QA
> protocol that wraps this checklist: freeze the RC, test the *packaged* binary, capture a
> forensic baseline, run the abuse batteries (crash mid-op, filesystem failures, download abuse,
> user-ownership conflicts, degraded network, upgrade/migration), and the hard-stop conditions.
> This file is the per-feature "does it work" battery; RC_TESTING is the "prove it's safe under
> abuse before we ship" gate. Cowork works the RC as a hostile QA engineer, not to turn boxes green.

Run top-to-bottom in the built app before tagging a release. Each item: **do → expect**.
Tags: `[app]` app-only · `[game]` needs FS25 (and often a loaded save) · `[net]` needs the
SiloAPI catalog reachable. "Auto-verified" = the logic/data path is covered by Rust unit
tests / `svelte-check` in CI; the box here is the *human* GUI/behaviour check that code
can't do.

Launch for testing: from `Silo/`, `npm run tauri:dev` (leave the terminal open — closing it
closes the app). A packaged build is `npm run tauri:build`.

> **Revised for v0.1.1.** The entire frontend was refactored into focused
> components/panels — so **re-run every section**: a change that compiles and builds can
> still have a runtime wiring regression. Two earlier bugs should now pass on re-check:
> §6 Browse thumbnails (were blank — CSP fix) and §10 Bindings map (stuck on "Reading…" —
> dedup fix). Plus the new **§14 Security & reliability** checks below.

---

## 0. Launch & shell
- [x] `[app]` App launches, no error dialog, window renders the Library view.
- [x] `[app]` The sidebar switches views cleanly (Library, Browse, Mod sets, Diagnose, Updates, Tools, Settings, Help); theme (Settings → General) switches System / Light / Dark and both themes are legible.
- [x] `[app]` Resize the window — no horizontal scroll, panels stay centered.

## 1. Library scan & organize  _(auto-verified: scan/organize/moddesc engines)_
- [x] `[app]` Initial scan lists the full library; counts (the status strip's "N of M active", the toolbar's maps / script mods) look right; Settings → About shows the last scan time ("scanned in N ms").
- [x] `[app]` Library → **⋯** → Rescan re-reads without error; warm scan is fast.
- [x] `[app]` Organize moves loose zips into `mods/archive/<Category>/`; dev **folder** mods are left untouched (zip-only).
- [ ] `[app]` **Organize preview**: the Library status strip's **N new mods aren't filed → Review** (auto-file OFF; Settings → Library → Library layout → **Organize** works too) opens a dry-run preview grouping the planned moves by category **before** anything moves; **Cancel** touches nothing; **Organize N** applies it. Ticking **"Don't preview next time"** and confirming makes the next Organize skip straight to applying (clear `localStorage silo.organizePreviewSkip` to re-enable).
- [x] `[app]` Category rail filters; search filters by title/author/tech name; Favorites/Hidden toggles work.
- [x] `[app]` Select-all checkbox activates/deactivates the filtered set.

## 1b. Mods folder selection  _(auto-verified: modsroot resolve/override/path-map, per-folder DB isolation, migration)_
- [ ] `[app]` First launch after upgrading: the library looks exactly as before (same active set, no "foreign file" errors); Settings → Mods folder shows **FS25 default**.
- [ ] `[game]` Turn on FS25's mods directory override to a scratch folder (`gameSettings.xml`: `active="true"`). Launch Silo → it uses that folder and shows the one-time "Silo now uses the mods folder FS25 is set to" notice.
- [ ] `[app]` Settings → Mods folder → **Change…** → pick another folder → confirm "Point Farming Simulator at this folder too?" → `gameSettings.xml` now points there and `gameSettings.xml.bak` exists.
- [ ] `[game]` Launch FS25 → the mods from the new folder load.
- [ ] `[app]` Switch back via **Other folders Silo knows** → the original folder's active set is intact.
- [ ] `[app]` Choose a folder and decline the game update → the "FS25 is reading a different mods folder" notice shows; **Point FS25 here** clears it.
- [ ] `[app]` Rename the chosen folder while Silo is closed → launch → the "isn't available" notice shows and nothing is scanned; renaming it back + **Try again** recovers.
- [ ] `[linux]` (tester) Silo finds the Proton prefix user folder; an override set in-game round-trips.

## 1c. Library archive location  _(auto-verified: relocate move/pause/resume/cancel/adopt state machine, Layout, link policy, process matcher)_

Never on the real library — switch to a **disposable mods folder** (§1b) with ~20 copied mods, and
capture the forensic baseline from `RC_TESTING.md` first.

- [ ] `[app]` Settings → Library → **Library archive** shows the path labelled *Inside your mods folder (default)*. **Change…** → pick a folder on **another drive** → the confirm dialog shows the mod count, size and free space → **Move library**. The move notice shows progress; when it finishes the setting reads *Custom location*, the library lists every mod, and the old archive's now-empty folders are cleaned up. Forensic before/after diff: every change explained, no mod lost, no `.silo-move-tmp` / `.silo-move-old` left behind.
- [ ] `[app]` **Use default** moves the library back to `<mods folder>/archive` the same way.
- [ ] `[app]` **Same-drive move frees space:** with a few large mods **active**, move the archive to another folder on the **same drive** as the mods folder → afterwards the drive's free space is about what it was before (not short by the active mods' size), each active mod in the mods folder still loads, and moved mods keep their original modified dates.
- [ ] `[game]` **Pause → play:** start a move, **Pause** mid-move, then **Launch** from Silo → FS25 starts and the active mods load (an active mod already moved still loads). With FS25 still running, **Resume** is refused with "Close Farming Simulator 25 first…". Close FS25 → **Resume** finishes the move.
- [ ] `[game]` **Game starts mid-move:** start a move, then launch FS25 yourself (not through Silo) → the move pauses itself within a few seconds, nothing is half-moved, and **Resume** is refused until FS25 is closed.
- [ ] `[app]` **Cancel mid-move:** start a move, **Cancel move** → the mods Silo already moved go back; the archive tree matches the baseline (paths + sizes) and the setting is unchanged.
- [ ] `[app]` **While a move is unfinished** (running or paused): Organize, activate/park, Reclaim disk space, mods-folder switching and the Settings archive controls are disabled, each with "Finish or cancel the archive move first" written under it; **auto-file** does nothing; **Launch** is blocked while running and works while paused.
- [ ] `[app]` **Adopt:** point the archive at a folder that already holds loose `.zip` mods (include one whose techName is already in the library) → after the move, the Organize preview opens in adopt mode → applying it files the new mods under `<archive>/<Category>/` as parked; the duplicate is listed as already in your library and left untouched in the folder.
- [ ] `[app]` **Archive unavailable — not forgotten:** with a custom archive set, close Silo, unplug/rename the archive drive, relaunch → the "Your library archive isn't available" notice shows, parked mods are hidden, nothing is deleted. **Rescan** a couple of times while it's still missing. Reconnect (or rename it back) → **Try again** → every parked mod is listed again, still parked in its category — the library was not forgotten. While it's missing, Organize / Change… preflight / **Use default** as a move are refused with "Your library archive at … isn't available…".
- [ ] `[app]` **Archive unavailable — re-point:** with the archive still missing, copy its folder (with its `<Category>/` subfolders) somewhere else → the notice's **Choose another** → pick the copy → the confirm says nothing is moved → **Use this folder** → the notice clears, the library rescans and lists every mod from the copy as parked; activate one and confirm it lands in the mods folder. Then unplug again and use Settings → Library → **Use default** → the confirm says it starts fresh and the old drive's mods stay untouched → afterwards the archive reads *Inside your mods folder (default)*, the old folder is unchanged on disk, and nothing was copied anywhere. Activating / parking a mod that was linked from the old drive works (its stale link is replaced). **Way back:** reconnect the old drive → Settings → Library → Library archive → **Change…** → pick the old archive → the move is refused (it holds filed mods) but Silo offers **Use this folder** (moves nothing) → the library lists the old archive's mods again.
- [ ] `[app]` **Cross-drive copies notice:** with the archive on another drive where links aren't possible (Windows without Developer Mode), Settings → Maintenance → **Reclaim disk space** reports "Your library archive is on another drive and Windows can't link across drives without Developer Mode — active mods stay as copies." and changes nothing. The Change… confirm dialog also suggests Developer Mode.

**Release gate** — these need a real game and can't be unit-tested. Windows is required before
shipping; macOS and Linux are experimental platforms, verified by testers as they report in:
- [x] `[game]` **Windows, Developer Mode on:** FS25 loads **symlinked active zips** (archive on another drive; the mods folder entry is a symlink into the archive). _Passed 2026-10-17 for v0.9.0: 6 zips symlinked from `E:` into a mods folder on `F:`; `log.txt` listed all 6 as available mods with their hashes and versions._
- [ ] `[game]` **macOS, Steam install:** FS25 loads symlinked active zips from an archive on another volume.
- [ ] `[game]` **macOS, Mac App Store install:** the mods folder is inside the app's sandbox container, so active zips and **folder (unzipped) mods** are **copied, not linked** — confirm they load and no symlink is created.
- [ ] `[game]` **Linux / Proton:** FS25 loads symlinked active zips (ask the #166 reporter to verify on CachyOS).
- [x] If symlinked zips do **not** load on Windows, `supports_symlinks` must return `false` on Windows before release (copy only). _Not needed: they load._

## 2. Curation & detail drawer  _(auto-verified: curation/tags/rating store)_
- [x] `[app]` Click a mod row → detail drawer opens with icon, metadata, deps (with present/missing status).
- [x] `[app]` Favorite / Broken / Hidden toggles persist across a rescan; star rating persists; tags add/remove & filter; notes save on blur.
- [ ] `[app]` **Integrity verification**: in a mod's detail drawer, **Verify integrity** returns one of **Verified** (a GitHub-source mod SiloAPI has hashed, e.g. `FS25_CabView`), **Modified** (edit a file inside a verified mod's zip → expect the changed file listed), or **Unverified** (a ModHub-only-latest or unknown mod — correct, not a bug). Folder mods show "can't be verified". No red console errors.
- [ ] `[app]` **Last run** section shows this mod's log health (see §8) — errors/cosmetic/clean.
- [ ] `[net]` **Catalog** section shows latest version / "update available (source)" / "not in catalog".
- [ ] `[app]` Reveal opens the file in Explorer; activate/deactivate toggles the active dot.

## 3. Loadouts & savegames  _(auto-verified: loadout/savegame parse)_
- [ ] `[app]` Save current active set as a loadout; apply/overwrite/delete (Mod sets → My sets); the matching loadout's name shows in the Library status strip and at the top of the Play menu ("…, unchanged").
- [ ] `[app]` Export a loadout to `.silo`, re-import it as a new loadout.
- [ ] `[game]` Mod sets → Savegames lists real saves with map + mod list; "→ Loadout" builds a loadout from a save's mods (warns on missing).
- [ ] `[app]` Savegame backup copies to `SiloBackups/`.

## 4. Conflicts  _(auto-verified: conflict engine incl. map + fillType)_
- [ ] `[app]` Diagnose → Conflicts (its tab badge shows the count) lists uniqueType / specialization / script collisions with severity + involved mods.
- [ ] `[app]` **Duplicate-map**: enable 2+ maps → a critical "N maps enabled" conflict appears; drop to one → it clears.
- [ ] `[app]` **fillType override**: if two active mods declare the same `<fillType name>`, a warning appears naming both; distinct filltypes do NOT flag.

## 5. Health & launch
- [ ] `[app]` Diagnose → Problems: missing deps, corrupt mods, digit-prefix names. The header status chip counts them (and critical conflicts) as problems; clicking it opens Diagnose.
- [ ] `[game]` **Play** in the header starts FS25 (Steam) with the active set projected.

## 6. Browse (catalog)  _(auto-verified: adapters, /mods shape)_
- [ ] `[net]` Browse loads the catalog; header shows total; **scroll works** (regression-fixed).
- [ ] `[net]` Search filters; category dropdown filters; "Load more" pages to the end ("That's all N").
- [ ] `[net]` Per-source buttons render with versions: GitHub = ⬇ (install), ModHub = ↗ (open page). "In library" badge on owned mods.
- [ ] `[net][game]` Install a GitHub mod → progress bar advances (MB), lands in the library on rescan, then loads in-game.
- [ ] `[net]` Details opens the drawer: cover image, facts, full source list with per-source versions + open-page links.

## 7. Updates
- [ ] `[net]` Updates (sidebar) → **Check for updates** checks the whole library via the catalog; rows show current→latest with source badge; Install works.
- [ ] `[net]` Detail-drawer GitHub link: paste owner/repo (or accept the auto-suggestion), Check, Install — backs up `.bak`.

## 8. Crash & log triage  _(auto-verified: log parser on real 713-mod log)_
- [ ] `[app]` Diagnose → Last run → verdict banner: healthy / crashed / errors-worth-a-look, matching your last run.
- [ ] `[app]` Culprit mods ranked; cosmetic-only mods behind the "safe to ignore" disclosure; unattributed count + Open log.txt.
- [ ] `[app]` Re-read log after a new FS25 run updates the report.

## 9. Guided bisection  _(auto-verified: stepper convergence 1..64)_
- [ ] `[app]` From Diagnose → Last run (crash, no culprit) or the footer link → intro shows suspect count + ~launches.
- [ ] `[game]` Full loop: Start → Silo applies half → Launch FS25 → reproduce → quit → "still broken?" (crash auto-detected) → narrows → repeats → names a culprit (or "interaction").
- [ ] `[app]` Finish/cancel restores your original active set exactly.
- [ ] `[app]` **Crash-safety**: start a bisection, force-quit Silo mid-run, relaunch → the interrupted-diagnosis notice offers to restore your set; Restore my mod set works.

## 10. Bindings map  _(auto-verified: parser on real inputBinding.xml)_
- [ ] `[app]` Tools → Controls map lists devices; every action↔key shown; search filters; "inputs bound to more than one action" section highlights reuse (as review, not error).

## 11. Multiplayer sync  _(auto-verified: manifest diff + md5 vs md5sum)_
- [ ] `[app]` Mod sets → Multiplayer → Export active set → saves a `.silomp`.
- [ ] `[app]` Round-trip: Verify against your own just-exported manifest → "your set matches."
- [ ] `[app]` Negative: deactivate a mod, re-verify the same manifest → it shows under "get these" (and reactivating clears it). Change a version → "wrong version". An extra active mod → "turn these off".

## 12. Filltype bridge  _(auto-verified: generator XML vs real mods)_
- [ ] `[app]` Tools → Fill type bridge → enter a filltype name, pick categories, Generate → `.zip` appears in mods folder + rescan; it shows in the library.
- [ ] `[game]` **The real test**: enable the bridge + the map, load a save → the filltype now works with the checked equipment (haul/store/sell). If off, delete the zip (reversible).

## 13. Config editors & diagnostics  _(auto-verified: xmlconfig/settings_form)_
- [ ] `[game]` Tools → Graphics settings → **Edit game.xml**: apply a preset, relaunch → setting took; `.bak` written.
- [ ] `[game]` Savegame config editor (Mod sets → Savegames → Configure): change difficulty/growth etc., load save → applied; `.bak` written.
- [ ] `[app]` Per-mod settings form (for a mod that has settings) edits + saves with `.bak`; raw-XML fallback works.
- [ ] `[app]` Diagnostics report exports a markdown summary (Settings → Diagnostics → **Export report**).

---

## 14. Security & reliability (v0.1.1 hardening)
- [ ] `[app]` **Token in the OS keychain.** Connect GitHub (Settings → Accounts). Then check the OS credential store — Windows: `Credential Manager → Windows Credentials`, look for `com.hllmr.silo`; macOS: Keychain Access, search "Silo". The token should appear there. It should NOT be readable in the app's `silo.db` (it lives in `app_data_dir`).
- [ ] `[app]` **Token persists + still works.** Restart the app: still "Connected as …", and a source action (⭐ Star / 👍 Endorse) still works.
- [ ] `[app]` **Disconnect clears it.** Disconnect GitHub → the `com.hllmr.silo`/`gh_token` entry is gone from the credential store.
- [ ] `[net]` **Corrupt-update guard.** (If practical) point an update at a truncated/non-zip file → it errors ("isn't a valid .zip") and does NOT replace the existing mod; no `.silo-update.part` is left beside it.
- [ ] `[net]` **Updates keep the old version.** Update a GitHub-linked mod (active and organized). The mod shows the new version, the game's mods folder entry loads it, and `mods/backups/` holds `<file>.<secs>.update.bak` with the old one. After four updates of the same mod only the newest three backups remain. With FS25 running, the update is refused ("Close Farming Simulator 25 first, then update.").
- [ ] `[net]` **A killed update recovers.** Start updating a large mod and kill Silo (Task Manager) during the download, and once more right after it finishes. Relaunch: the mod works and is either the old version (download thrown away) or the new one with the old in `backups/`; no `.silo-update.part` or `.silo-update-old` is left, and any rollback or finish is reported in a notice in the corner.
- [ ] `[app]` **Quitting mid-change asks first.** Start an Organize of the whole library (or an archive move) and close the window (and, on macOS, Cmd+Q) while it runs → "Silo is still changing files… Quit anyway?" appears once. "Keep Silo open" keeps it running; "Quit anyway" quits. With nothing running, closing quits at once.
- [ ] `[app]` **Organize is non-destructive.** After an Organize + Flatten round-trip, no mod files are missing (count returns to baseline). If Silo reports "kept an unrecognized file in archive/", it did NOT delete it — that's the safety guard working.
- [ ] `[app]` **User replacements are never deleted.** Organize a mod, deactivate it (parked), then manually drop a *different* file with the same name into the flat `mods/` folder. Re-activate then deactivate (or Flatten): Silo must **leave your file untouched** and report "isn't the one Silo projected", never delete it.
- [ ] `[app]` **Scans stay fast + safe.** A full rescan of the large library completes without error (zip-read caps don't reject normal mods).

## 15. Full UI coverage sweep — open every panel, drawer, toggle, modal

The frontend was split into ~20 new components, so this section exists to **touch every
interactive surface at least once** and catch a wiring regression that compiles fine.
For each item: it **opens/toggles without error and renders its content**; overlays close
cleanly (their ✕ or Esc) and re-opening works. **Keep the devtools console open
(if available) and confirm no red errors appear** as you go — a broken prop or `$effect` in
an extracted component shows up there first. Nothing should cover the header (the drawers
start below it).

**Sidebar and header (always visible; see §19 for the full map):**
- [ ] `[app]` Every sidebar item opens its view; leaving the Library **closes an open detail drawer**.
- [ ] `[app]` **Mod sets** → My sets (loadouts), Share (collections), Multiplayer (MP sync), Savegames tabs each render.
- [ ] `[net]` **Updates** opens the Updates page; **Check for updates** in its header runs the check.
- [ ] `[app]` Library → **⋯** → **Rescan** runs a scan (progress bar → counts refresh).
- [ ] `[app]` **Settings** opens as a page with tabs like Tools: General, Mods folder, Library, Accounts, Diagnostics, Maintenance, About. The tab you left on is the one it reopens on (until a restart, when it starts on General).
- [ ] `[game]` **Play** is present (don't have to run it here); its chevron opens the launch menu.

**Library status strip, toolbar and filters:**
- [ ] `[app]` Tools → **Library stats** shows the stats; Diagnose → **Conflicts** and **Problems** list conflicts and health issues; Diagnose → **Last run** is the crash/log triage; Tools → **Controls map** and **Fill type bridge** open their tools.
- [ ] `[app]` Filter toggles each flip and re-filter the list: **Favorites**, **Hidden**, **⚑ Flagged**, **⚠ In conflict** (disabled when 0). Combine two, then clear. The toolbar's **maps / script mods / uniqueType / active** counts scope the list one at a time (click again to clear).
- [ ] `[app]` The header **search** filters the Library live; clearing (Esc) restores the list.

**Library toolbar + rail:**
- [ ] `[app]` **Category rail**: click a category and a subcategory → list narrows; "All mods" resets.
- [ ] `[app]` **Right-click a category** → context menu appears and dismisses.
- [ ] `[app]` **Sort** dropdown cycles Name / Category / Size / Recently added / Version / **My rating**; the **↑/↓** direction button flips order.
- [ ] `[app]` **Select-all** checkbox activates/deactivates the filtered set (indeterminate state renders when partial).

**Library detail drawer (click a mod row) — every sub-section:**
- [ ] `[app]` Header (icon/title/author/version/tech name) + **✕ closes** (Esc too); **drag the left edge to resize** (persists); the drawer docks beside the list, which stays clickable (clicking another mod switches the drawer).
- [ ] `[app]` Action buttons all respond: **Active/Parked**, **Favorite**, **Broken**, **Hidden**, **Reveal**, and **Settings** (only when the mod has settings).
- [ ] `[app]` **Category editor**: change the dropdown/subcategory → **Save** enables; **Reset to auto** shows for an overridden mod.
- [ ] `[app]` **Last run** + **Catalog** status render (the `ModStatus` component).
- [ ] `[app]` **Rating / tags / notes** (the `ModCuration` component): set a star, add + remove a tag, type a note (saves on blur).
- [ ] `[net]` **GitHub link** (`ModRepoLink`): the owner/repo field, Check, and Install controls render.
- [ ] `[app]` **Dependencies**, **uniqueType**, and **Conflicts** sub-sections render when present.

**Settings page — every control:**
- [ ] `[app]` **General**: **Theme** System / Light / Dark (both themes legible); **UI size**; **Graphics settings → Edit game.xml**.
- [ ] `[app]` **Mods folder**: the path in a read-only field with **Open folder** and **Change…** on the right; the game install below it.
- [ ] `[app]` **Library**: **Organize**, the **Auto-file new mods** switch and **Library archive** are present. Confirm auto-file is **OFF by default** on a fresh profile.
- [ ] `[app]` **Accounts**: **GitHub account**: Connect / **Enable actions** / **Use a Personal Access Token** / Disconnect all render (see §14 for the real connect).
- [ ] `[app]` **Diagnostics**: **Copy diagnostics** and **Export report**.
- [ ] `[app]` **Maintenance**: **Re-read every mod**, **Reclaim disk space** (organized, not cloud-synced) and **Undo organizing** (red). Undo organizing asks "Put every mod back in your mods folder?" with the **Undo organizing** button; Cancel changes nothing. While a change runs, each off button says why in text under it.
- [ ] `[app]` **About**: the version and the last scan time; with a Silo update ready, **Restart to update** (the same action as the update notice); the links open in the browser.

**Browse tab — every surface:**
- [ ] `[net]` **Sort** dropdown (Popular / Most downloaded / Top rated / Newest / Name) and **category** dropdown both re-query.
- [ ] `[net]` A **card** shows title/author/rating/downloads badges + per-source chips; **Details** opens the Browse drawer.
- [ ] `[net]` **Browse drawer**: facts, the full **Available from** source list, and the **Interact** cards render — **GitHub** (★/⑂/👁/◎ + Star/Watch), **ModHub** (⭐ + Rate ↗).
- [ ] `[net]` **Read more** opens the **description modal** (full body + "Open full mod page"); closes.

**Every remaining modal opens + closes cleanly:**
- [ ] `[app]` Savegame config editor · game.xml editor · Per-mod settings form · Organize preview · Category menu · Row context menu · Diagnostics "Export report".

## 16. Look, type & accessibility (design foundations, #224)
- [ ] `[app]` **Focus ring.** Press Tab through the sidebar, the header and then a mod row (the active toggle button, name, row actions) → a visible outline is on every control as you reach it, in both the light and dark themes. Clicking a button with the mouse shows no outline (text fields show it, as they should).
- [ ] `[app]` **UI size.** Settings → General → Appearance → UI size: switch to 90%, 110% and 125% → text and icons scale together; nothing clips or overlaps (header, sidebar, mod rows, drawer, Settings). Back to 100% restores the original look, and the choice survives a restart.
- [ ] `[app]` **Fonts.** Headings (the "Silo" title, panel titles) render in Fraunces and body text in Hanken Grotesk — not Georgia or Segoe UI. Works with the network off.
- [ ] `[app]` **Icons.** Icons sit beside their words (sidebar, Settings, drawer actions); every icon-only button shows a tooltip on hover. No leftover symbol characters (⛓ ⟳ ⚑ ◆ and the like) where an icon should be.
- [ ] `[app]` **Both themes readable.** In light and in dark, secondary (muted) text, warning text and links are easy to read on their backgrounds; gold is never used for text in the light theme.
- [ ] `[app]` **Silo mark.** The sidebar shows the silo mark (green tile, cream silo, gold sun) and it looks the same in both themes.

## 17. Dialogs, menus and loadout safety (#226)  _(auto-verified: dialog/menu/toast component tests, loadout name check + switch diff, confirm build guard)_
**Keyboard, in every in-app dialog:**
- [ ] `[app]` **Focus starts in the right place.** A destructive confirm (Delete loadout, Delete collection, Forget the interrupted diagnosis) opens with focus on **Cancel**; any other confirm (Switch, Move library, Undo organizing) opens with focus on the action button; a name dialog (Loadouts → Save current active set as a loadout) opens with focus in the text field.
- [ ] `[app]` **Tab stays inside.** Tab and Shift+Tab cycle through the dialog's controls only; nothing behind the dialog can be reached or clicked.
- [ ] `[app]` **Esc cancels.** Esc closes the dialog and the action does **not** happen (no loadout saved, switched or deleted; nothing moved).
- [ ] `[app]` **Focus comes back.** After Cancel or Esc, focus is back on the button that opened the dialog.
- [ ] `[app]` **Buttons say what they do.** No dialog has an "OK" button; each confirm button names its action ("Delete", "Switch", "Replace", "Move library").
- [ ] `[app]` **One click answers one dialog.** Double-click a confirm button whose answer opens another confirm (or hold Enter on it) → only the first dialog is answered; the next one stays open until you click it again. A name dialog's **Replace** also needs a fresh click or Enter after it appears.

**Loadouts:**
- [ ] `[app]` **Save, empty name.** Loadouts → **+ Save current active set as a loadout**, leave the name empty and press Save → an error under the field ("Give the loadout a name"); the dialog stays open and nothing is saved.
- [ ] `[app]` **Save, duplicate name.** Type the name of an existing loadout (try different capitals) → a warning that it already exists and the button changes to **Replace**. Press Replace → that loadout now holds your current mods, and the list still has **one** loadout of that name, not two. A notice offers **Undo**; pressing it puts the loadout's old mods back.
- [ ] `[app]` **Switch shows what changes.** Pick a loadout that differs from your active mods → the dialog shows counts (Turn on / Park, plus Not in your library when some are missing) and the list of mods that change. **Cancel** → your active mods are exactly as before.
- [ ] `[app]` **Switch, then Undo.** Switch again and confirm → the loadout's mods are active and a notice says "Switched to …" with **Undo** (or "Switch to … finished with problems." when some mods couldn't be applied; the error shows too). Press Undo → your previous active mods are back (check the active count and a few rows).
- [ ] `[app]` **Undo refuses when things moved on.** Switch, then turn one mod on or off, then press the switch notice's Undo → a notice says it can't undo because your active mods changed, and nothing changes. Switching twice leaves only the newest switch's Undo notice.
- [ ] `[app]` **Already in use.** Pick the loadout that matches your active mods → no dialog, just an "Already using …" notice.
- [ ] `[game]` **Use a savegame's mods.** Savegames → "→ Loadout" on a save → the same what-changes dialog, plus a note about mods the save used that aren't in your library; Switch then Undo restores your previous active mods.
- [ ] `[app]` **Overwrite asks.** Press a loadout's **Overwrite with current active set** button → a dialog says how many mods it had and how many are added and removed. Cancel leaves the loadout unchanged; Replace updates it and doesn't change your active mods, and a notice offers **Undo**, which puts the loadout's old mods back.
- [ ] `[app]` **Delete, then Undo.** Press a loadout's **Delete loadout** button → a dialog with focus on Cancel. Delete → it leaves the list and a notice offers **Undo**. Press Undo → the loadout is back with the same name and mods.

**Cancel truly cancels, and the quit guard:**
- [ ] `[app]` **Library move.** Start moving the library archive, then press **Cancel move** in the move notice → the "Cancel the library move?" dialog; choose **Keep going** → the move carries on. (Choosing Cancel move puts moved mods back.)
- [ ] `[net]` **Delete a collection.** Delete a collection and press **Cancel** (or Esc) → it is still in the list, on your GitHub, and its share link still works.
- [ ] `[app]` **Quit guard is still a Windows dialog.** Close Silo while it's moving files → a Windows dialog (not Silo's in-app one) asks "Quit anyway?"; Keep Silo open keeps it running.

**Notices (toasts):**
- [ ] `[app]` **They don't steal focus.** When a notice appears, focus stays where it was (typing in a field carries on uninterrupted). Tab can still reach its Undo button.
- [ ] `[app]` **Not behind a dialog.** With an Undo notice on screen, open any confirm dialog → clicking the notice's Undo or close button does nothing until the dialog is closed.
- [ ] `[app]` **They wait for you.** Switch to a loadout, then alt-tab away for 20 seconds and come back → the Undo notice is still there and Undo still works. Pointing at a notice also keeps it on screen.

## 18. Notices, toasts and activity (#228)  _(auto-verified: notice keys, notice builders, write lock on a control, background vs foreground ops, the queued rescan, error copy body rule, toast slots, activity trimming, dialog fallback)_

One notice area under the header replaces the stacked banners; one-off results and failures are toasts; the bell in the header lists both.

- [ ] `[app]` **Archive drive unplugged.** With a custom library archive, close Silo, unplug the archive's drive, relaunch → one red notice "Your library archive isn't available: …" with **Try again** and **Choose another**, and no close button. Reconnect → **Try again** → the notice goes and the library lists the parked mods again.
- [ ] `[app]` **A rescan keeps other warnings.** With a notice up (unplugged archive, or a mods folder FS25 doesn't read), press **Rescan** twice and alt-tab away and back → the notice is still there. Several notices at once show the most important one, every red one and any with a progress bar, with "+N more" for the rest.
- [ ] `[app]` **A failed organize says why in plain words.** Open a loose mod zip in 7-Zip (keep it open), then **Organize** → a red toast "Silo couldn't move FS25_….zip because another program is using it. …" with **Try again** and **Copy details**; it stays until closed. Close 7-Zip → **Try again** → the mod is filed. **Copy details** puts the version, time, operation, code and raw message on the clipboard.
- [ ] `[app]` **The activity bell.** After the failed organize the bell in the header has a red dot; open it → the failure is listed (with Copy details) along with recent toasts and finished operations, and the dot clears. While a move or a long Reclaim disk space runs, the bell shows a blue dot and the running operation.
- [ ] `[app]` **One library change at a time.** Run **Reclaim disk space** (Settings → Maintenance) and, while it runs, look at Library → ⋯ → Rescan, Play, Settings' folder and layout controls and a mod's Install button → they're off, pointing at Play says what Silo is doing ("Silo is optimizing links.") and each Settings control shows that reason as text under it; the strip's **Review** (unfiled mods) is refused with that reason. Trying a second change some other way (a mod's toggle) is refused with a warning toast giving the same reason; nothing changes.
- [ ] `[app]` **A library move leaves the rest of Silo usable.** Start a library move and, while it runs, open Settings (**Copy diagnostics** works), Mod sets → Savegames (**Back up** works), My sets, Share, Multiplayer and Updates, and use Library → ⋯ → **Rescan** → all open and work; Play stays off and the strip's **Review** is refused. A change (a mod's toggle, a loadout apply, an update's Install) is refused with a warning toast "Silo is moving your library. Try again when it's done."; nothing changes.
- [ ] `[app]` **A paused move refuses changes plainly.** Pause the move, then toggle a mod or apply a loadout → a warning toast "Finish or cancel the library move first. You can resume or cancel it from the notice at the top."; nothing changes. Resume or cancel from the notice → changes work again.
- [ ] `[app]` **Try again waits for a running change.** With a red mods-folder notice up (a chosen mods folder on an unplugged drive), start a change that runs a while → the notice's **Try again** is off and pointing at it says what Silo is doing; when the change ends it's on again, and using it rescans and lists the folder's mods. (A scan asked for while a change runs is queued and runs once it ends; auto-verified.)
- [ ] `[app]` **Startup messages stay.** When Silo restored its database (or reports something else at startup), it shows as an info notice with a close button that stays until dismissed, not a toast that fades.
- [ ] `[app]` **Results are toasts, not timed bars.** With every mod current, press **Needs update** → an "All mods are up to date" toast (no strip that disappears on its own). Settings → Maintenance → **Reclaim disk space** → the status line under the header shows "Optimizing links… n/N" while it runs, then a toast with the result.
- [ ] `[app]` **A view's crash stays in that view.** In normal use no "This part of Silo hit a problem." panel ever appears. (Dev build only: make a page or panel throw while rendering → only that part shows the fallback with **Reload this view** and **Copy details**, in place on the page; an overlay (mod settings, a settings editor, the Organize preview, the dialog host) shows it as a card centred in the window with **Close** as well. The rest of Silo keeps working. If the dialog host itself fails, confirmations fall back to the system dialog.)

## 19. Navigation (#230)  _(auto-verified: view outlet routing, keyboard shortcuts, tabbed views follow nav, status chip / strip / Play menu copy, toolbar facets)_

Sidebar: Library · Browse · Mod sets · Diagnose · Updates, then Tools · Settings · Help. Header: search, status chip, activity bell, Play.

- [ ] `[app]` **Every sidebar item opens its view** with its title; the last view is remembered across a restart, except Browse (quit on Browse → Silo reopens on the Library and makes no catalog call). Opening Updates doesn't check by itself. Below 1200 px the sidebar collapses to icons (each has a tooltip) and its toggle pins it. After using the arrow keys in the sidebar and tabbing away, Tab back lands on the current page's item.
- [ ] `[app]` **Everything is still reachable** (old place → new place):
  - Library / Browse tabs → sidebar **Library** / **Browse**.
  - Savegames, Loadouts, Collections, Multiplayer buttons → **Mod sets** → Savegames / My sets / Share / Multiplayer. A `silo://collection?url=…` link opens **Mod sets → Share** pre-filled.
  - ⟳ Updates → sidebar **Updates** (badge = updates found) → **Check for updates**; the strip's **N updates → See updates**.
  - Organize N → the strip's **N new mods aren't filed → Review** (and Settings → Library → Organize).
  - Rescan → Library **⋯** → Rescan; Settings' Rebuild → Library **⋯** → Re-read every mod (also Settings → Maintenance → **Re-read every mod**).
  - Launch → **Play** (header); Settings (gear) → sidebar **Settings**; activity bell → header.
  - Self-update button → the **Silo X is ready → Restart to update** notice (always shown, never under "+N more"), and Settings → About (**Restart to update**).
  - Stat bar: mods → Tools → Library stats; conflicts → Diagnose → Conflicts; need attention → Diagnose → Problems; diagnose → Diagnose → Last run; bindings → Tools → Controls map; bridge → Tools → Fill type bridge; maps / script mods / uniqueType / active filters → the Library toolbar; scanned in N ms → Settings → About.
  - Settings → Graphics (game.xml) → **Tools → Graphics settings**; Settings → General → Graphics settings → **Edit game.xml** opens the same editor directly.
  - Browse → a source card's connect link → **Settings → Accounts**, scrolled to and focused on **GitHub account** (it does it again on a second request; clicking the Accounts tab yourself leaves focus on the tab).
  - Settings controls by tab: **General** → theme (System / Light / Dark), UI size, Graphics settings → Edit game.xml; **Mods folder** → the folder (Open folder, Change…, Use the game's folder), other folders Silo knows (Switch), the game install (Open folder); **Library** → Library layout (Organize), Auto-file new mods, Library archive (Open folder, Change…, Use default); **Accounts** → GitHub account (and the no-keychain warning); **Diagnostics** → Copy diagnostics, Export report; **Maintenance** → Re-read every mod (was Rebuild categories), Reclaim disk space (was Optimize links), Undo organizing (was Restore vanilla / Restore flat folder); **About** → version, last scan, Restart to update, links (Website, Source code, Changelog, Discord, GitHub Discussions).
- [ ] `[app]` **The drawer docks.** Open a mod's details → the drawer sits right of the list, which stays scrollable and clickable, with no horizontal scrollbar in the drawer; clicking another mod switches it; ✕ / Esc closes it; leaving the Library closes it. Drag its left edge → it resizes, and the grab handle stays on the edge when a notice appears or goes and when the sidebar collapses.
- [ ] `[app]` **Play guards still apply.** During a foreground change (Reclaim disk space) and while a library move is copying, Play is off and pointing at it says why; with a critical conflict, Play asks "Launch with N critical conflicts?" first. The chevron opens the launch menu (active set line, Open the game folder, savegames missing mods → Mod sets → Savegames, Manage mod sets) even while Play is off.
- [ ] `[app]` **Status chip.** "Checking…" while the first scan runs, "Not read yet" before any scan, "Couldn't read library" (amber) when the scan fails (point Silo at a folder it can't read); after a good scan "All good" with nothing to report, "N problems" (red) for critical conflicts or **active** mods that can't load, "N to review" for warning conflicts, foreign files or inactive mods that can't load; clicking it opens Diagnose. The Library strip says "Ready to play · …" only when Play would work; with a critical conflict, a blocked Play or no game found it starts with "N of M active".
- [ ] `[app]` **Loadouts wait for a change.** During **Reclaim disk space**, Mod sets → My sets → Save current, Replace (↻), Delete (✕) and Import are off and pointing at them says what Silo is doing; during a library move they work.
- [ ] `[app]` **A diagnosis survives switching pages.** Diagnose → Last run → start a guided diagnosis, then open Conflicts, the Library and Settings → the run isn't cancelled (no restore banner), Diagnose in the sidebar shows a dot (a screen reader reads "Diagnose, finding the problem mod"), and Last run shows the same round when you come back.
- [ ] `[net]` **GitHub sign-in survives navigation.** Settings → Accounts → GitHub account → Connect, then go to the Library and authorize on github.com → a toast "Connected to GitHub as <you>"; back in Settings it shows Connected. Start again and press **Cancel sign-in** → the code goes and nothing more is polled.
- [ ] `[app]` **Header search follows the view.** In the Library it filters the list; in Browse it searches the catalog; elsewhere Enter takes the text to the Library.
- [ ] `[app]` **Keyboard shortcuts** _(auto-verified: keymap resolution, typing / IME / dialog gates, F6 cycling and ? in a jsdom shell)_:
  - **Ctrl+K** focuses the header search from anywhere, even while typing in another field; **/** does too when you're not typing (and isn't typed into the box), also on layouts where / needs Shift.
  - **F5**, **Ctrl+R** and **Ctrl+Shift+R** rescan the library (the progress bar runs) and **never reload the window**, including mid-scan, during a library move, while typing in a field and with a dialog open. Behind a dialog, the Organize preview, a settings editor, a mod's settings or a right-click / category menu they do nothing at all (and no other shortcut fires). While a foreground change runs they don't rescan, same as Library **⋯** → Rescan.
  - **Alt+1 … Alt+5** open Library, Browse, Mod sets, Diagnose, Updates; **Ctrl+,** opens Settings; **Alt+←** goes back to the previous page. Typing digits or letters in a field never switches pages.
  - **F6** cycles focus rail → header (search) → notices (when any are showing) → main → drawer (when open) → rail; **Shift+F6** goes the other way.
  - **Esc** closes the mod drawer (or Browse's drawer) and focus goes back to the list (not lost to the window); with a dialog, a **⋯** menu or a right-click menu open over a drawer, Esc closes only that and the drawer stays open.
  - **?** opens the Keyboard shortcuts dialog (Help → **Keyboard shortcuts** opens the same one); typing **?** in the search box stays text. With the dialog open, no other shortcut fires.
  - A screen reader announces the shortcut on the search box (Control+K /), the sidebar items (Alt+1…5, Settings Control+,) and Library **⋯** → Rescan (F5).

## Sign-off
- [ ] All `[app]` items pass.
- [ ] All `[game]`/`[net]` items pass (or known-limitations noted below).
- [ ] `npm run tauri:build` produces a launchable packaged app.

**Known limitations to note at release:** ModHub are index-only (open-page, no direct
install — GIANTS gate downloads); ghost-keybind cleanup deferred (unverified premise);
override-conflict detection is fillTypes-only for now.
