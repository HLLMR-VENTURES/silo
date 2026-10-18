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
- [ ] `[app]` **Organize dialog**: the Library status strip's **N new mods aren't filed → Review** (auto-file OFF; Settings → Library → Library layout → **Organize** works too) opens the Organize dialog with a summary of the dry run and the planned moves grouped by category under **See every file**, **before** anything moves; **Cancel** touches nothing; **File N mods** applies it (see §25 for the auto-file box, progress and partial failures).
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
- [x] `[app]` Favorite / Flag / Hidden toggles persist across a rescan; star rating persists; tags add/remove & filter; notes save on blur.
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
- [ ] `[net]` Updates (sidebar) → **Check now** checks the whole library via the catalog; rows show current → latest with a source chip; **Update** works (see §22).
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
- [ ] `[net]` **Updates** opens the Updates page; **Check now** in its header runs the check.
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
- [ ] `[app]` Action buttons all respond: **Active/Parked**, **Favorite**, **Flag** (reads **Flagged** when on), **Hidden**, **Reveal**, and **Settings** (only when the mod has settings).
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
- [ ] `[app]` Settings → Diagnostics → **Open log folder** opens the folder holding `silo.log` (Windows `%LOCALAPPDATA%\com.hllmr.silo\logs`) in the file manager.
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
- [ ] `[app]` **Focus ring.** Press Tab through the sidebar, the header and then into the Library list (one Tab stop: the row itself, see §20) → a visible outline is on every control as you reach it, in both the light and dark themes. Clicking a button with the mouse shows no outline (text fields show it, as they should).
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
- [ ] `[app]` **One library change at a time.** Run **Reclaim disk space** (Settings → Maintenance) and, while it runs, look at Library → ⋯ → Rescan, Play, Settings' folder and layout controls and a mod's Install button → they're off, pointing at Play says what Silo is doing ("Silo is reclaiming disk space.") and each Settings control shows that reason as text under it; the strip's **Review** (unfiled mods) is refused with that reason. Trying a second change some other way (a mod's toggle) is refused with a warning toast giving the same reason; nothing changes.
- [ ] `[app]` **A library move leaves the rest of Silo usable.** Start a library move and, while it runs, open Settings (**Copy diagnostics** works), Mod sets → Savegames (**Back up** works), My sets, Share, Multiplayer and Updates, and use Library → ⋯ → **Rescan** → all open and work; Play stays off and the strip's **Review** is refused. A change (a mod's toggle, a loadout apply, an update's Install) is refused with a warning toast "Silo is moving your library. Try again when it's done."; nothing changes.
- [ ] `[app]` **A paused move refuses changes plainly.** Pause the move, then toggle a mod or apply a loadout → a warning toast "Finish or cancel the library move first. You can resume or cancel it from the notice at the top."; nothing changes. Resume or cancel from the notice → changes work again.
- [ ] `[app]` **Try again waits for a running change.** With a red mods-folder notice up (a chosen mods folder on an unplugged drive), start a change that runs a while → the notice's **Try again** is off and pointing at it says what Silo is doing; when the change ends it's on again, and using it rescans and lists the folder's mods. (A scan asked for while a change runs is queued and runs once it ends; auto-verified.)
- [ ] `[app]` **Startup messages stay.** When Silo restored its database (or reports something else at startup), it shows as an info notice with a close button that stays until dismissed, not a toast that fades.
- [ ] `[app]` **Results are toasts, not timed bars.** With every mod current, press **Needs update** → an "All mods are up to date" toast (no strip that disappears on its own). Settings → Maintenance → **Reclaim disk space** → the status line under the header shows "Reclaiming disk space… n/N" while it runs, then a toast with the result.
- [ ] `[app]` **A view's crash stays in that view.** In normal use no "This part of Silo hit a problem." panel ever appears. (Dev build only: make a page or panel throw while rendering → only that part shows the fallback with **Reload this view** and **Copy details**, in place on the page; an overlay (mod settings, a settings editor, the Organize preview, the dialog host) shows it as a card centred in the window with **Close** as well. The rest of Silo keeps working. If the dialog host itself fails, confirmations fall back to the system dialog.)

## 19. Navigation (#230)  _(auto-verified: view outlet routing, keyboard shortcuts, tabbed views follow nav, status chip / strip / Play menu copy, toolbar facets)_

Sidebar: Library · Browse · Mod sets · Diagnose · Updates, then Tools · Settings · Help. Header: search, status chip, activity bell, Play.

- [ ] `[app]` **Every sidebar item opens its view** with its title; the last view is remembered across a restart, except Browse (quit on Browse → Silo reopens on the Library and makes no catalog call). Opening Updates doesn't check by itself. Below 1200 px the sidebar collapses to icons (each has a tooltip) and its toggle pins it. After using the arrow keys in the sidebar and tabbing away, Tab back lands on the current page's item.
- [ ] `[app]` **Everything is still reachable** (old place → new place):
  - Library / Browse tabs → sidebar **Library** / **Browse**.
  - Savegames, Loadouts, Collections, Multiplayer buttons → **Mod sets** → Savegames / My sets / Share / Multiplayer. A `silo://collection?url=…` link opens **Mod sets → Share** pre-filled.
  - ⟳ Updates → sidebar **Updates** (badge = updates found) → **Check now**; the strip's **N updates → See updates**.
  - Organize N → the strip's **N new mods aren't filed → Review** (and Settings → Library → Organize).
  - Rescan → Library **⋯** → Rescan; Settings' Rebuild → Library **⋯** → Re-read every mod (also Settings → Maintenance → **Re-read every mod**).
  - Launch → **Play** (header); Settings (gear) → sidebar **Settings**; activity bell → header.
  - Self-update button → the **Silo X is ready → Restart to update** notice (always shown, never under "+N more"), and Settings → About (**Restart to update**).
  - Stat bar: mods → Tools → Library stats; conflicts → Diagnose → Conflicts; need attention → Diagnose → Problems; diagnose → Diagnose → Last run; bindings → Tools → Controls map; bridge → Tools → Fill type bridge; maps / script mods / uniqueType / active filters → the Library toolbar; scanned in N ms → Settings → About.
  - Settings → Graphics (game.xml) → **Tools → Graphics settings**; Settings → General → Graphics settings → **Edit game.xml** opens the same editor directly.
  - Browse → a source card's connect link → **Settings → Accounts**, scrolled to and focused on **GitHub account** (it does it again on a second request; clicking the Accounts tab yourself leaves focus on the tab).
  - Settings controls by tab: **General** → theme (System / Light / Dark), UI size, Graphics settings → Edit game.xml; **Mods folder** → the folder (Open folder, Change…, Use the game's folder), other folders Silo knows (Switch), the game install (Open folder); **Library** → Library layout (Organize), Auto-file new mods, Library archive (Open folder, Change…, Use default); **Accounts** → GitHub account (and the no-keychain warning); **Diagnostics** → Open log folder, Copy diagnostics, Export report; **Maintenance** → Re-read every mod (was Rebuild categories), Reclaim disk space (was Optimize links), Undo organizing (was Restore vanilla / Restore flat folder); **About** → version, last scan, Restart to update, links (Website, Source code, Changelog, Discord, GitHub Discussions).
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

## 20. Library list and cards (#242)  _(auto-verified: keyed virtual list/grid, roving focus and keys, row switch name / parked text / capped badges, badge order, view preference, filter predicates and chips, Needs attention, selection model, BulkBar, >25 confirm, switch Undo and its staleness refusal, askWithOption, loose-park explanation once and its plural, the bulk filing sequence and its dialog, Undo re-offered when refused, per-row bulk flag saves, focus hand-off when a row, the BulkBar or the last chip goes, duplicate keys, the BulkBar's arrow keys, density setting)_

- [ ] `[app]` **List by default.** With no saved choice (fresh profile, or clear `silo.library.view` from localStorage) the Library opens as a list, for any library size. Switch to **Card view** in the toolbar, restart → it reopens on cards; switch back → list again.
- [ ] `[app]` **Row content.** Each row: icon, title (a ★ after it when it's a favorite), then "author · version · category · size"; on the right at most three badges then "+n" (pointing at "+n" lists the rest), problems first (Modified, Can't load, Ignored name, Conflict, Flagged), then status (Verified build, Update, Folder mod, Hidden), then info (Map, Has settings, N scripts); the Active switch with the word **Active** or **Parked**; and an always-visible **⋯**. Below ~640 px of list width the badges give way to the title.
- [ ] `[app]` **Parked rows.** A parked mod's icon is grey and the word "Parked" is muted, but its title and meta text are full strength (not faded) in both themes. Hidden mods (with **Hidden** on) show a Hidden badge instead of fading.
- [ ] `[app]` **Switch.** Clicking the switch activates / parks the mod (the drawer doesn't open); while a library move or other foreground change runs it says why and nothing changes. A screen reader names it "Active: <mod name>, switch, on/off".
- [ ] `[app]` **One Tab stop.** Tab from the toolbar lands on one row (the last one you were on, else the first on screen); the next Tab leaves the list. **↑/↓** move between rows, **Home/End** jump to the first/last mod (the list scrolls to keep it in view, with a little room), **PageUp/PageDown** move a screen. Scroll the focused row far away with the wheel and press ↓ → focus continues from it, never from the top of the window.
- [ ] `[app]` **Row keys.** On a focused row: **Enter** opens the drawer; **Space** turns the mod on or off; **Shift+F10** or the Menu key opens the row's menu under the row; Esc closes it and focus is back on the row. Shift+↑/↓ and Ctrl+Space select (see Bulk select below).
- [ ] `[app]` **⋯ and right-click are the same menu.** Activate / Park, Open details, Find in Browse, Change category…, Edit settings… (mods with settings), Favorite / Remove favorite, Hide / Unhide, Flag / Remove flag. Each does what it says; Change category… opens the category menu next to the row.
- [ ] `[app]` **Checkbox.** Hovering or focusing a row shows a checkbox at its left; clicking it selects the row (see Bulk select below). The row's text doesn't move when it appears.
- [ ] `[app]` **Drawer hands focus back.** Open a mod with Enter, scroll the list far away, press Esc (or ✕) → the list scrolls back to that mod's row and it has focus. With the mod filtered out meanwhile, focus goes to the page instead (never lost to the window).
- [ ] `[app]` **Cards.** Card view shows the same switch, ⋯ menu, ≤3 badges and hover checkbox, a grey image for parked mods, and 1 column below 600 px, 2 at the regular width, more on a wide window. **←/→** move by one card, **↑/↓** by a row, Enter / Space / Shift+F10 as in the list.
- [ ] `[app]` **Show: All / Active / Parked.** The segment above the list shows every mod, only the active set or only parked mods; the count ("N shown") follows. Restart → the choice is kept (`silo.library.state`).
- [ ] `[app]` **Favorites and Needs attention.** Both chips toggle (a screen reader says "pressed"). Needs attention shows mods with a problem badge (Can't load, Ignored name, Conflict, Modified, Flagged) or an update; its count is on the chip.
- [ ] `[app]` **Filters popover.** **Filters** opens a panel with Hidden, Flagged, In conflict (off while there are no conflicts), Has settings, Needs update (checks for updates the first time; says "All mods are up to date" and stays unticked when there's nothing), Maps, Script mods, Declares a uniqueType, and a Tag select when you have tags. The button counts what's ticked ("Filters (2)"). Esc closes it and focus is back on the button. The old row of six toggle buttons and the "Active" select-all checkbox are gone.
- [ ] `[app]` **Filter chips.** Every filter in effect (including the segment, a tag and the category) shows as a chip under the toolbar; its ✕ ("Remove filter: Maps") removes just that one, **Clear filters** removes them all. A tag picked from a mod's drawer shows as "Tag: …".
- [ ] `[app]` **Empty states.** Filter down to nothing → "No mods match these filters." with **Clear filters** (it also clears the search). With an empty mods folder → "No mods in this folder yet." with **Browse mods**, **Open mods folder** and **Choose another folder**, each doing what it says.
- [ ] `[app]` **Bulk select.** Click a row's checkbox → it's selected and every row's checkbox stays visible; the toolbar becomes "1 selected · Turn on · Park · Favorite · Hide · Select all N shown · Clear". **Shift+click** another checkbox selects everything between; **Ctrl+click** (or **Ctrl+Space** on a row) adds or removes one; **Shift+↑/↓** on a focused row extends from the anchor and shrinks again when you go back. **Select all N shown** selects every row in view; **Clear**, or **Esc** with focus in the list, clears it (Esc in an open menu or dialog closes only that). Changing a filter drops selected mods that are no longer shown.
- [ ] `[app]` **Bulk Turn on / Park.** With a few mods selected, **Park** parks them and a toast says "Parked 3 mods." with **Undo**; **Turn on** likewise. With more than 25 to change, a dialog asks first ("Park 40 mods?", with the count); Cancel changes nothing. While a library move or other change runs, Turn on / Park are off and say why. **Favorite** / **Hide** set the flag on every selected mod (Remove favorite / Unhide when they all already are), with Undo.
- [ ] `[app]` **Undo after a switch.** Flip one mod's switch → "Parked <name>." (or "Turned on <name>.") with **Undo** for about 10 s; Undo puts the active set back. Flip a second mod → the first toast goes away (one switch toast at a time). Flip a mod, then change another one by hand, then press the first toast's Undo → "Can't undo: your active mods changed since the switch." and nothing changes. A loadout switch's toast behaves the same.
- [ ] `[app]` **Parking a loose mod, explained once.** With a loose .zip (not filed yet, auto-file off), park it → "Park <name>?" explains it moves into your library folder (archive\<category>) and stays in the library, with **Don't explain this again**. Park it → it's filed and the Undo toast shows. Park another loose mod in the same session → no dialog, just the toast. Tick **Don't explain this again** once → it never asks again, even after a restart (`silo.explain.parkLoose`); without the tick it asks again next launch. Cancel → nothing moves.
- [ ] `[app]` **Bulk Park with loose mods.** With auto-file off, select a few mods including two or more loose .zips (not filed yet) and press **Park** → a dialog every time, even after ticking "Don't explain this again" on a single park: "Park N mods?", saying how many aren't filed yet and that Silo moves them into your library folder (archive\<category>, or each into its category folder), with the counts (Park, Filed into the library: n loose, plus "Library folder: On another drive" when the archive is on another drive) and the loose mods listed (up to 50, then "…and n more"). With more than 25 to change it's still one dialog. Cancel → nothing moves. Park → the loose mods are filed into the library and the rest are parked; the toast "Parked N mods." has **Undo**, which puts them all back in the mods folder. If a library move or other change is running, it says why before any dialog and nothing changes.
- [ ] `[app]` **Undo while busy.** Park a few mods, start another library change (Settings → Library → Reclaim disk space, say), then press the switch toast's Undo → it says Silo is busy and the Undo toast comes back; press it again once the change is done → the mods are back.
- [ ] `[app]` **Esc order.** With mods selected and a mod's drawer open, with focus in the list: the first Esc clears the selection, the second closes the drawer (focus stays on the row). With focus in the drawer, Esc closes only the drawer.
- [ ] `[app]` **Focus isn't lost.** Focus a row, then change a filter so that mod disappears → focus is on the row now in its place (the last row if it was the last). Select mods, press **Clear** in the selection bar → focus is back on your row. Remove the last filter chip with its ✕ → focus is on **Filters**. In the selection bar, ←/→ move between its buttons and Tab leaves it. Focus a row, switch to **Card view** → the same mod is the tab stop and in view (and back again). A screen reader names a row "<mod name>, active" or "…, parked".
- [ ] `[app]` **List density.** Settings → General → **List density: Comfortable / Compact** changes the Library list's row height at once; restart → kept (`silo.library.density`).
- [ ] `[app]` **Fast with a big library.** On the 700+ mod library, scrolling the list and holding ↓ stay smooth; icons fill in as rows appear.

## 21. Mod detail drawer (#245)  _(auto-verified: header verdict badges and their order and spoken explanations, stale catalog answers ignored, no Update for a versionless mod on the catalog's word, Modified file count, Problem from a critical conflict or a crashed last run, Update from the drawer's own catalog check, the ⋯ menu items and labels, Change category… focuses the picker, every per-mod control has a name, focus to the title on open and on switching mods (not on a rescan), Esc closes only with focus in the drawer and only the ⋯ menu when it's open, focus back to the row by key)_

- [ ] `[app]` **Order.** Open a mod's details: header (icon, name, "author · v1.2.0.0", badges, ✕), then the Active switch, Settings (mods with editable settings) and **⋯**, then any banners, then About (catalogued mods), Details, Integrity, Source and updates, My notes, Category. No "vv" in any version.
- [ ] `[app]` **Verdict badges.** An unchecked mod has no integrity badge. **Verify integrity** on a ModHub mod you haven't touched → **Verified build** appears in the header at once; on a mod you edited → **Modified (n files)** with the right count. A mod with a newer catalog version (or flagged by Check for updates) shows **Update available**. A mod in a critical conflict (two active maps) shows **Problem**. Each badge has an icon and a word, and pointing at it explains it.
- [ ] `[app]` **Primary row.** The switch turns the mod on or parks it (the words beside it follow); Settings opens its settings; a mod whose settings aren't written yet shows a line saying so instead. **⋯** has Favorite / Remove favorite, Flag / Remove flag, Hide / Unhide, Change category… (scrolls to Category and focuses the picker), Show file (opens the folder with the file selected) and Find in Browse; each does what it says and the labels follow the mod's state.
- [ ] `[app]` **Banners.** Drop a different build of a managed mod into the mods folder → "A different build is in your mods folder" with **Use this version** and **Keep Silo's v…**, both work. A mod that logged errors in the last game → "Logged N errors in your last game" with a sample line and **See in Diagnose** (opens Diagnose → Last run). A mod in a warning or critical conflict → "Conflicts with N active mods", each conflict listed in plain words (Exclusive slot, Fill type, Map…), **See conflicts** opens Diagnose → Conflicts. No banner when all is well.
- [ ] `[app]` **Details.** Category, size, type, shop items and scripts (when any), multiplayer, Last run (with the log's sample line for warnings), **Mod ID** in mono with a copy button (paste it somewhere: it's the tech name), **Exclusive slot** (mods with a uniqueType), and "Needs N mods" with each dependency marked In your library or Missing.
- [ ] `[app]` **Everything still there.** Integrity (verdict, file lists, "Provenance check, not a virus scan."), Silo's catalog status with Get it (or View source when there's no download), Where you got it (save, open, change, remove, I checked it), GitHub repo (link, check, open, install), rating, tags (click to filter the Library, ✕ to remove), notes, and the category override with Reset to automatic.
- [ ] `[app]` **Read more.** A catalogued mod with a long description shows **Read more**; it opens the full text in a dialog, Esc closes only the dialog and focus is back on Read more.
- [ ] `[app]` **Focus.** Open a mod with Enter or a click → focus is on its name (a screen reader reads it). Tab walks every control in order down the drawer; Shift+Tab walks back. Press Enter on another row in the list → the drawer shows it and focus is on its name. Rescan while typing a note → focus stays in the note.
- [ ] `[app]` **Esc.** With focus anywhere in the drawer, Esc closes it and focus is on the mod's row (scrolled back into view if you scrolled away). With focus in the header search, Esc doesn't close the drawer. With the ⋯ menu or a dialog open, Esc closes only that. F6 moves between the list and the drawer.
- [ ] `[app]` **Width.** Drag the drawer's left edge → it resizes; restart → the width is kept.

## 22. Updates (#244)  _(auto-verified: section grouping and order, rows updated since the check leaving the sections, rail badge only after a check, Not tracked only when the catalog answered, the check's catalog + linked-repo merge and progress, Couldn't check reasons, Update all refused / partial / stopped / success and its summary, ModHub dropped-build detection once per build, the auto-check offer shown once and its persisted setting)_

- [ ] `[app]` **Never checked.** Fresh profile (or clear `silo.updates.*` from localStorage) → Updates shows "Check your mods for updates" with **Check now** and the line "Checking sends the names of your installed mods to Silo's catalog. Versions are compared on your computer." The header says "Mod updates · Not checked yet". The rail's Updates item has no badge. Opening Updates doesn't check by itself.
- [ ] `[app]` **Offered once.** The first visit shows "Check for mod updates when Silo starts?" with **Check on start** and **Only when I ask**. Leave without answering, come back → it isn't shown again. Settings → General → Updates → **Check for mod updates when Silo starts** is off.
- [ ] `[net]` **Check on start.** Turn the setting on (or answer **Check on start**), restart Silo → a few seconds after the library loads, the rail badge appears with the number of newer versions, without opening Updates and without installing anything. Turn it off, restart → no check runs (no catalog call) until you press **Check now**.
- [ ] `[net]` **Check now.** Press it → a progress line ("Asking Silo's catalog…", then "Checked 3 of 12 linked GitHub mods" if you've linked repos) and the header says "Checking…", then "Last checked just now" (it ticks to "1 minute ago" while open).
- [ ] `[net]` **Sections, in order, empty ones hidden.** **Ready to install (n)** (GitHub releases, each "8.0.0.4 → 8.0.1.0" with a GitHub chip and **Update**), **Download from ModHub (n)** with its help text and **Open on ModHub**, **Check these yourself (n)** (saved source pages: **Open page**, **I checked it**), **Couldn't check (n)** (the reason, **Try again**, **Copy details**), then **Up to date (n)** and **Not tracked (n)**, both collapsed; Not tracked explains that saving a page in a mod's details moves it to Check these yourself.
- [ ] `[net]` **Rail badge.** After a check the Updates badge = Ready to install + Download from ModHub (no badge when that's 0). It drops by one as each mod is updated.
- [ ] `[net]` **Update one.** Press **Update** on a row → it says "Updating…", then a toast "Updated <mod> to <version>. The previous version is in backups." with **Show** (opens the backup in the file manager); the row moves to Up to date. With FS25 running → the row shows "Close Farming Simulator 25 first, then try again." with **Try again** and **Copy details**; close the game and Try again works.
- [ ] `[net]` **Update all.** With two or more in Ready to install, press **Update all N** → no dialog; the header line reads "Updating 1 of N: <mod>", "Updating 2 of N: …"; each row says "Updating…" in turn; at the end one toast "Updated N mods. The previous versions are in backups." While it runs, Update / Update all / other library changes are off and say why under the heading. With one release asset broken (or the network pulled mid-way) → the rest still update and the toast says "3 updated, 1 failed."; the failed row keeps its reason. With FS25 running → it stops after the first ("0 updated, 1 failed. N not tried.", "Close Farming Simulator 25 first, then try again.").
- [ ] `[net]` **Check and update don't overlap.** While a check runs, Update and Update all are off ("Wait for the check to finish." under the heading); while an update runs, Check now is off ("Wait for the updates to finish."). The previous results and badge stay up during a re-check.
- [ ] `[app]` **Busy library.** Start a library move (or Reclaim disk space) and press **Update all** → refused with "Silo is … Try again when it's done."; nothing is downloaded.
- [ ] `[net]` **ModHub: open, then use.** For a ModHub row press **Open on ModHub** → the mod's page opens in your browser. Download the new zip and save it over the mod's file in your mods folder, then switch back to Silo → a toast "Found a newer <mod> (<version>)." with **Use this version**, and the row shows "Found <version> in your mods folder." with **Use this version**. Press it → Silo keeps the new build (the old one goes to backups) and the row moves to Up to date. The toast appears once per saved build. An older build (save an earlier zip over it) gets no toast and no button.
- [ ] `[net]` **Catalog offline.** Disconnect the network and press **Check now** → "Silo can't reach its catalog right now. Your library still works." with **Try again** and **Copy details**; Not tracked doesn't appear (Silo can't tell). Reconnect, Try again → the sections fill.
- [ ] `[app]` **Silo's own update** is still the "Silo x.y.z is ready" notice (Restart to update), separate from mod updates.

## 23. Browse (#246)  _(auto-verified: spelled-out source chips, card height following UI size, Load more never mixing two searches, the latest drawer request winning, keyboard drawer focus and its return, Find in Browse opening the drawer, a malformed author claim ignored, install state per card and its words, Verified build vs Verified author badges (author only with a claim), the badge cap, catalog error copy, the empty-search title, columns from width, skeleton cards, offline state with Try again, Clear search, keyed card list, Open on ModHub, the install-progress listener dropped on unmount even when it arrives late, GitHub sign-in polling after the Settings view unmounts, the catalog's verifiedAuthor passing through Rust)_

- [ ] `[net]` **Cards.** Browse shows cards in a grid that adds columns as the window widens (one below ~480 px of grid width). Each card: cover, title, author, rating, downloads, the sources spelled out with their own versions ("GitHub 1.0.6.0", "ModHub 1.0.5.0"; never "GH" / "MH"), at most two badges then "+n" (pointing at it lists the rest), and one button at the bottom. Mods you already have are not faded; they carry an **In your library** badge.
- [ ] `[net]` **Fast with the whole catalog.** Load more a few times (300+ cards), scroll fast → smooth, covers fill in as cards appear, no blank flashes of whole rows.
- [ ] `[net]` **One Tab stop.** Tab from the filters lands on one card; the next Tab leaves the grid. Arrows move between cards (↑/↓ by a row), Home/End jump to the ends. **Enter** opens the card's drawer and focus moves to its title (Tab goes on to its buttons); **Esc** (or ✕) closes it and focus is back on that card. Opening with the mouse, scrolling the grid away and closing doesn't scroll the grid back. A screen reader names a card "<title> by <author>" (", in your library" when you have it).
- [ ] `[net]` **Install.** On a GitHub-installable card the button says **Install**; click it → it becomes "Installing… 42%" with a bar (or "Installing…" until the size is known), other cards' Install buttons wait ("Wait for the current install to finish."), then a toast "Installed <title>." with **Show in library**, which jumps to the mod after the rescan. The card then says **Show in library** and shows **In your library**.
- [ ] `[net]` **Update available.** After Updates found a newer version of a mod you have (Updates → Check now), its Browse card shows an **Update** badge and the button says **See update**, which opens Updates. The drawer offers **See update** and **Show in library** (the GitHub row's button also says **Show in library**).
- [ ] `[net]` **ModHub.** A ModHub-only card's button says **Open on ModHub** (hover: why Silo opens the page) and opens the mod's page in your browser. In the drawer the ModHub row's button says **Open on ModHub**.
- [ ] `[net]` **Badges.** A mod whose build Silo's catalog hashed shows **Verified build** (green, shield icon). **Verified author** (sky blue, badge-check icon) shows only on mods the catalog says an author claimed; until SiloAPI's author program ships, no card shows it. The two never share a colour or icon. The drawer header shows the same badges.
- [ ] `[app]` **Offline.** Disconnect the network (or block silo-api.hllmr.com) and open Browse → "Silo can't reach its catalog right now." / "Your library still works." with **Try again** and **Copy details**. The Library and everything else keep working. Reconnect, press **Try again** → the cards load (and the mod count line comes back).
- [ ] `[net]` **Empty search.** Search the header for something absurd ("zzzzlizard") → "No mods match “zzzzlizard”." with **Clear search**, which empties the header search and shows the catalog again. With a filter on too, **Clear filters** is there as well.
- [ ] `[net]` **Loading.** On a slow connection (or the first visit), Browse shows grey skeleton cards (still under reduced motion), never a bare "Loading…" line.
- [ ] `[net]` **Load more.** "Load 60 more" under the grid adds the next page; at the end "That's all N." If loading more fails, "Silo couldn't load more mods." with **Try again**, and the cards already shown stay.
- [ ] `[app]` **No leaked listeners.** Switch between Library and Browse ten times, then install a mod from Browse → its progress updates once per event (DevTools: no growing count of `install:progress` listeners).

## 25. First run, Help and What's new (#250)  _(auto-verified: setup flow vs What's new gating from the database (flag, organized mods, curation rows) and a failed read showing neither, only the two known flags writable, the three steps and their focus, Skip setup / Not now moving nothing, planning waits for the scan, the mods-folder variants (not created, override, unavailable, layout problem, mismatch, cloud-synced, game not found), another drive using the picked folder or showing why not, organize progress, partial failure with Copy details, a refused organize staying put, the Organize dialog's auto-file box, footer progress and failed rows, Help search with "loadouts" finding Mod sets, Report a problem copying before it opens the form (and opening when the copy fails), the third-party list with both fonts' OFL texts and Lucide's ISC text)_

Use a **fresh** profile for the first-run items: rename `silo.db` in Silo's app-data folder (`%APPDATA%\com.hllmr.silo` on Windows) and put it back afterwards. Test against a copy of a real mods folder.

- [ ] `[app]` **Setup shows for a new library only.** With no `silo.db`, Silo opens full-window: the Silo wordmark top left, **Skip setup** top right, three steps (Your mods / How Silo keeps them / Organize). The sidebar and header are hidden behind it and keyboard shortcuts (`?`, Ctrl+K, F5) do nothing.
- [ ] `[app]` **Step 1, the folder.** "Silo found your mods folder", the path, "Farming Simulator 25 reads this folder", and "Reading 312 of 737 mods…" counting up, then "Found 737 mods.". **Choose another folder…** opens Silo's folder picker and switches (the count restarts). **This is right** moves on.
- [ ] `[app]` **Step 1 variants.** Each in turn: a custom FS25 folder (`modsDirectoryOverride` in gameSettings.xml) → "Farming Simulator is set to read `D:\FS25 Mods`. Silo will use that folder too."; a chosen folder on an unplugged drive → "Silo can't open `E:\Mods`. Is the drive connected?" with **Try again**, **Choose another…**, **Use the game's folder**; FS25's mods folder deleted → "Farming Simulator hasn't made its mods folder yet" with **Create it** / **Choose a folder…**; a cloud-synced folder → the copy-instead-of-link line; game uninstalled or not found → "Silo didn't find Farming Simulator 25…".
- [ ] `[app]` **Step 2, where parked mods live.** The Active / Parked shelves, "Parking a mod never deletes it.", the archive choice (inside, recommended, showing `…\mods\archive`) and the auto-file box. **On another drive…** opens the folder picker: cancel → the choice goes back to Inside; a folder inside the mods folder → the reason shows inline and the choice goes back; a good empty folder → its path shows under the option and Settings → Library later shows it as the archive (nothing was moved). Picking **Inside** again goes back to the default.
- [ ] `[app]` **Step 3, organize.** "Ready to organize 737 mods into 14 categories", the biggest categories with counts, the three ✓ lines, **See every file** listing every move by category. Nothing has moved yet (check the mods folder). **Organize 737 mods** → "Filing 412 of 737…" with a bar → "Your library is organized. 737 mods filed, 737 active, nothing deleted." → **Go to my library** shows the Library with every mod still active, and FS25 loads them (the mods folder holds links/copies of each).
- [ ] `[app]` **Auto-file from setup.** With the step 2 box ticked and step 3 organized, Settings → Library → Auto-file new mods is on; drop a new zip into the mods folder and refocus Silo → it's filed. With the box ticked but **Not now** chosen, auto-file stays off.
- [ ] `[app]` **Partial failure in setup.** Lock one zip (open it in 7-Zip) before organizing → "Silo filed 736 of 737 mods. 1 couldn't be moved:" with the file and why, **Copy details** (the clipboard holds the list) and **Go to my library**; the locked mod is still loose and active in the Library; the activity bell lists the problem.
- [ ] `[app]` **Skip and Not now.** Fresh profile → **Skip setup** → the Library, nothing moved. Fresh profile → step 3 → **Not now, show my library** → same. Either way, restart Silo → setup doesn't come back (the flag is in the database: clearing the webview's storage doesn't bring it back either).
- [ ] `[app]` **What's new for an upgrade.** With your real (organized) `silo.db` and a build from before this change → update → the "What's new in Silo 1.0" card opens once over the Library (new layout, Mod sets, Undo, keyboard shortcuts, plain-English errors); **Got it** closes it; restart → it doesn't come back. The setup flow never appears for this library.
- [ ] `[app]` **Organize dialog.** Auto-file off, drop 3 zips into the mods folder, rescan → strip "3 new mods aren't filed → Review" → the dialog: summary for 3 mods, **See every file**, the box "File new mods automatically from now on". **File 3 mods** → "Filing 3 of 3…" in the footer → the dialog closes and a toast says "Filed 3 mods into your library. All active mods stay active." Repeat with the box ticked → Settings → Library shows auto-file on and the strip item no longer appears. "Don't preview next time" is gone, and an old `silo.organizePreviewSkip` no longer skips the dialog.
- [ ] `[app]` **Organize dialog partial failure.** Lock one of the zips → the dialog stays open: "Silo filed 2 of 3 mods. 1 couldn't be moved:", the row marked **Not moved** under See every file, **Copy details**, **Close**. The two filed mods are still active; the locked one is still loose.
- [ ] `[app]` **Help.** Help (?) in the sidebar: **Help topics** with a search box (type "loadouts" → only **Mod sets**; "zzz" → "Nothing in Help matches…"), **See what's new** (the 1.0 card again), **Keyboard shortcuts** (the ? dialog), Discussions / Discord, **Report a problem** (the toast "Diagnostics copied…" appears **before** the browser opens the GitHub bug form; paste → the diagnostics) and **Copy diagnostics only**, and **About Silo** with the version, the MIT line, Website / Source code / License, and **Third-party licenses** listing Fraunces and Hanken Grotesk (OFL-1.1, full text), Lucide (ISC), Svelte and the Tauri packages, each expanding to its text.
- [ ] `[app]` **No "beta", plain copy.** Walk every view and dialog: no "beta" anywhere, buttons are verbs in sentence case, and no em-dash asides in messages (error toasts, conflict descriptions, archive-move problems, organize failures).

## Sign-off
- [ ] All `[app]` items pass.
- [ ] All `[game]`/`[net]` items pass (or known-limitations noted below).
- [ ] `npm run tauri:build` produces a launchable packaged app.

**Known limitations to note at release:** ModHub are index-only (open-page, no direct
install — GIANTS gate downloads); ghost-keybind cleanup deferred (unverified premise);
override-conflict detection is fillTypes-only for now.
