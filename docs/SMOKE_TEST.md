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
- [x] `[app]` Library/Browse tabs switch cleanly; theme toggle (Settings) cycles system/light/dark and both themes are legible.
- [x] `[app]` Resize the window — no horizontal scroll, panels stay centered.

## 1. Library scan & organize  _(auto-verified: scan/organize/moddesc engines)_
- [x] `[app]` Initial scan lists the full library; counts (mods/maps/scripts/DLC) look right; "scanned in N ms" shows.
- [x] `[app]` Rescan re-reads without error; warm scan is fast.
- [x] `[app]` Organize moves loose zips into `mods/archive/<Category>/`; dev **folder** mods are left untouched (zip-only).
- [ ] `[app]` **Organize preview**: clicking **Organize N** (auto-file OFF) opens a dry-run preview grouping the planned moves by category **before** anything moves; **Cancel** touches nothing; **Organize N** applies it. Ticking **"Don't preview next time"** and confirming makes the next Organize skip straight to applying (clear `localStorage silo.organizePreviewSkip` to re-enable).
- [x] `[app]` Category rail filters; search filters by title/author/tech name; Favorites/Hidden toggles work.
- [x] `[app]` Select-all checkbox activates/deactivates the filtered set.

## 1b. Mods folder selection  _(auto-verified: modsroot resolve/override/path-map, per-folder DB isolation, migration)_
- [ ] `[app]` First launch after upgrading: the library looks exactly as before (same active set, no "foreign file" errors); Settings → Mods folder shows **FS25 default**.
- [ ] `[game]` Turn on FS25's mods directory override to a scratch folder (`gameSettings.xml`: `active="true"`). Launch Silo → it uses that folder and shows the one-time "Silo now uses the mods folder FS25 is set to" notice.
- [ ] `[app]` Settings → Mods folder → **Change…** → pick another folder → confirm "Point Farming Simulator at this folder too?" → `gameSettings.xml` now points there and `gameSettings.xml.bak` exists.
- [ ] `[game]` Launch FS25 → the mods from the new folder load.
- [ ] `[app]` Switch back via **Other folders Silo knows** → the original folder's active set is intact.
- [ ] `[app]` Choose a folder and decline the game update → the mismatch banner shows; **Point FS25 here** clears it.
- [ ] `[app]` Rename the chosen folder while Silo is closed → launch → the "isn't available" banner shows and nothing is scanned; renaming it back + **Retry** recovers.
- [ ] `[linux]` (tester) Silo finds the Proton prefix user folder; an override set in-game round-trips.

## 1c. Library archive location  _(auto-verified: relocate move/pause/resume/cancel/adopt state machine, Layout, link policy, process matcher)_

Never on the real library — switch to a **disposable mods folder** (§1b) with ~20 copied mods, and
capture the forensic baseline from `RC_TESTING.md` first.

- [ ] `[app]` Settings → **Library archive** shows the path labelled *Inside your mods folder (default)*. **Change…** → pick a folder on **another drive** → the confirm dialog shows the mod count, size and free space → **Move library**. The banner shows progress; when it finishes the setting reads *Custom location*, the library lists every mod, and the old archive's now-empty folders are cleaned up. Forensic before/after diff: every change explained, no mod lost, no `.silo-move-tmp` / `.silo-move-old` left behind.
- [ ] `[app]` **Use default** moves the library back to `<mods folder>/archive` the same way.
- [ ] `[app]` **Same-drive move frees space:** with a few large mods **active**, move the archive to another folder on the **same drive** as the mods folder → afterwards the drive's free space is about what it was before (not short by the active mods' size), each active mod in the mods folder still loads, and moved mods keep their original modified dates.
- [ ] `[game]` **Pause → play:** start a move, **Pause** mid-move, then **Launch** from Silo → FS25 starts and the active mods load (an active mod already moved still loads). With FS25 still running, **Resume** is refused with "Close Farming Simulator 25 first…". Close FS25 → **Resume** finishes the move.
- [ ] `[game]` **Game starts mid-move:** start a move, then launch FS25 yourself (not through Silo) → the move pauses itself within a few seconds, nothing is half-moved, and **Resume** is refused until FS25 is closed.
- [ ] `[app]` **Cancel mid-move:** start a move, **Cancel move** → the mods Silo already moved go back; the archive tree matches the baseline (paths + sizes) and the setting is unchanged.
- [ ] `[app]` **While a move is unfinished** (running or paused): Organize, activate/park, Optimize links, mods-folder switching and the Settings archive controls are disabled ("Finish or cancel the archive move first."); **auto-file** does nothing; **Launch** is blocked while running and works while paused.
- [ ] `[app]` **Adopt:** point the archive at a folder that already holds loose `.zip` mods (include one whose techName is already in the library) → after the move, the Organize preview opens in adopt mode → applying it files the new mods under `<archive>/<Category>/` as parked; the duplicate is listed as already in your library and left untouched in the folder.
- [ ] `[app]` **Archive unavailable — not forgotten:** with a custom archive set, close Silo, unplug/rename the archive drive, relaunch → the "Your library archive isn't available" banner shows, parked mods are hidden, nothing is deleted. **Rescan** a couple of times while it's still missing. Reconnect (or rename it back) → **Retry** → every parked mod is listed again, still parked in its category — the library was not forgotten. While it's missing, Organize / Change… preflight / **Use default** as a move are refused with "Your library archive at … isn't available…".
- [ ] `[app]` **Archive unavailable — re-point:** with the archive still missing, copy its folder (with its `<Category>/` subfolders) somewhere else → banner **Choose another** → pick the copy → the confirm says nothing is moved → **Use this folder** → the banner clears, the library rescans and lists every mod from the copy as parked; activate one and confirm it lands in the mods folder. Then unplug again and use Settings → **Use default** → the confirm says it starts fresh and the old drive's mods stay untouched → afterwards the archive reads *Inside your mods folder (default)*, the old folder is unchanged on disk, and nothing was copied anywhere.
- [ ] `[app]` **Cross-drive copies notice:** with the archive on another drive where links aren't possible (Windows without Developer Mode), Settings → **Optimize links** reports "Your library archive is on another drive and Windows can't link across drives without Developer Mode — active mods stay as copies." and changes nothing. The Change… confirm dialog also suggests Developer Mode.

**Release gate** — do not ship the archive location until each of these passes. They need a real game and can't be unit-tested:
- [ ] `[game]` **Windows, Developer Mode on:** FS25 loads **symlinked active zips** (archive on another drive; the mods folder entry is a symlink into the archive).
- [ ] `[game]` **macOS, Steam install:** FS25 loads symlinked active zips from an archive on another volume.
- [ ] `[game]` **macOS, Mac App Store install:** the mods folder is inside the app's sandbox container, so active zips and **folder (unzipped) mods** are **copied, not linked** — confirm they load and no symlink is created.
- [ ] `[game]` **Linux / Proton:** FS25 loads symlinked active zips (ask the #166 reporter to verify on CachyOS).
- [ ] If symlinked zips do **not** load on Windows, `supports_symlinks` must return `false` on Windows before release (copy only).

## 2. Curation & detail drawer  _(auto-verified: curation/tags/rating store)_
- [x] `[app]` Click a mod row → detail drawer opens with icon, metadata, deps (with present/missing status).
- [x] `[app]` Favorite / Broken / Hidden toggles persist across a rescan; star rating persists; tags add/remove & filter; notes save on blur.
- [ ] `[app]` **Integrity verification**: in a mod's detail drawer, **Verify integrity** returns one of **Verified** (a GitHub-source mod SiloAPI has hashed, e.g. `FS25_CabView`), **Modified** (edit a file inside a verified mod's zip → expect the changed file listed), or **Unverified** (a ModHub-only-latest or unknown mod — correct, not a bug). Folder mods show "can't be verified". No red console errors.
- [ ] `[app]` **Last run** section shows this mod's log health (see §8) — errors/cosmetic/clean.
- [ ] `[net]` **Catalog** section shows latest version / "update available (source)" / "not in catalog".
- [ ] `[app]` Reveal opens the file in Explorer; activate/deactivate toggles the active dot.

## 3. Loadouts & savegames  _(auto-verified: loadout/savegame parse)_
- [ ] `[app]` Save current active set as a loadout; apply/overwrite/delete; active loadout name shows in the toolbar.
- [ ] `[app]` Export a loadout to `.silo`, re-import it as a new loadout.
- [ ] `[game]` Savegames panel lists real saves with map + mod list; "→ Loadout" builds a loadout from a save's mods (warns on missing).
- [ ] `[app]` Savegame backup copies to `SiloBackups/`.

## 4. Conflicts  _(auto-verified: conflict engine incl. map + fillType)_
- [ ] `[app]` Conflicts stat shows count; panel lists uniqueType / specialization / script collisions with severity + involved mods.
- [ ] `[app]` **Duplicate-map**: enable 2+ maps → a critical "N maps enabled" conflict appears; drop to one → it clears.
- [ ] `[app]` **fillType override**: if two active mods declare the same `<fillType name>`, a warning appears naming both; distinct filltypes do NOT flag.

## 5. Health & launch
- [ ] `[app]` "Need attention" panel: missing deps, corrupt mods, digit-prefix names.
- [ ] `[game]` Launch button starts FS25 (Steam) with the active set projected.

## 6. Browse (catalog)  _(auto-verified: adapters, /mods shape)_
- [ ] `[net]` Browse loads the catalog; header shows total; **scroll works** (regression-fixed).
- [ ] `[net]` Search filters; category dropdown filters; "Load more" pages to the end ("That's all N").
- [ ] `[net]` Per-source buttons render with versions: GitHub = ⬇ (install), ModHub = ↗ (open page). "In library" badge on owned mods.
- [ ] `[net][game]` Install a GitHub mod → progress bar advances (MB), lands in the library on rescan, then loads in-game.
- [ ] `[net]` Details opens the drawer: cover image, facts, full source list with per-source versions + open-page links.

## 7. Updates
- [ ] `[net]` "⟳ Updates" checks the whole library via the catalog; rows show current→latest with source badge; Install works.
- [ ] `[net]` Detail-drawer GitHub link: paste owner/repo (or accept the auto-suggestion), Check, Install — backs up `.bak`.

## 8. Crash & log triage  _(auto-verified: log parser on real 713-mod log)_
- [ ] `[app]` "◆ diagnose" → verdict banner: healthy / crashed / errors-worth-a-look, matching your last run.
- [ ] `[app]` Culprit mods ranked; cosmetic-only mods behind the "safe to ignore" disclosure; unattributed count + Open log.txt.
- [ ] `[app]` Re-read log after a new FS25 run updates the report.

## 9. Guided bisection  _(auto-verified: stepper convergence 1..64)_
- [ ] `[app]` From triage (crash, no culprit) or the footer link → intro shows suspect count + ~launches.
- [ ] `[game]` Full loop: Start → Silo applies half → Launch FS25 → reproduce → quit → "still broken?" (crash auto-detected) → narrows → repeats → names a culprit (or "interaction").
- [ ] `[app]` Finish/cancel restores your original active set exactly.
- [ ] `[app]` **Crash-safety**: start a bisection, force-quit Silo mid-run, relaunch → recovery banner offers to restore your set; Restore works.

## 10. Bindings map  _(auto-verified: parser on real inputBinding.xml)_
- [ ] `[app]` "⌨ bindings" lists devices; every action↔key shown; search filters; "inputs bound to more than one action" section highlights reuse (as review, not error).

## 11. Multiplayer sync  _(auto-verified: manifest diff + md5 vs md5sum)_
- [ ] `[app]` "Multiplayer" → Export active set → saves a `.silomp`.
- [ ] `[app]` Round-trip: Verify against your own just-exported manifest → "your set matches."
- [ ] `[app]` Negative: deactivate a mod, re-verify the same manifest → it shows under "get these" (and reactivating clears it). Change a version → "wrong version". An extra active mod → "turn these off".

## 12. Filltype bridge  _(auto-verified: generator XML vs real mods)_
- [ ] `[app]` "⛓ bridge" → enter a filltype name, pick categories, Generate → `.zip` appears in mods folder + rescan; it shows in the library.
- [ ] `[game]` **The real test**: enable the bridge + the map, load a save → the filltype now works with the checked equipment (haul/store/sell). If off, delete the zip (reversible).

## 13. Config editors & diagnostics  _(auto-verified: xmlconfig/settings_form)_
- [ ] `[game]` game.xml graphics editor: apply a preset, relaunch → setting took; `.bak` written.
- [ ] `[game]` Savegame config editor: change difficulty/growth etc., load save → applied; `.bak` written.
- [ ] `[app]` Per-mod settings form (for a mod that has settings) edits + saves with `.bak`; raw-XML fallback works.
- [ ] `[app]` Diagnostics report exports a markdown summary.

---

## 14. Security & reliability (v0.1.1 hardening)
- [ ] `[app]` **Token in the OS keychain.** Connect GitHub (Settings). Then check the OS credential store — Windows: `Credential Manager → Windows Credentials`, look for `com.hllmr.silo`; macOS: Keychain Access, search "Silo". The token should appear there. It should NOT be readable in the app's `silo.db` (it lives in `app_data_dir`).
- [ ] `[app]` **Token persists + still works.** Restart the app: still "Connected as …", and a source action (⭐ Star / 👍 Endorse) still works.
- [ ] `[app]` **Disconnect clears it.** Disconnect GitHub → the `com.hllmr.silo`/`gh_token` entry is gone from the credential store.
- [ ] `[net]` **Corrupt-update guard.** (If practical) point an update at a truncated/non-zip file → it errors ("not a valid .zip archive") and does NOT replace the existing mod; a `.zip.bak` is only written when a real overwrite happens.
- [ ] `[app]` **Organize is non-destructive.** After an Organize + Flatten round-trip, no mod files are missing (count returns to baseline). If Silo reports "kept an unrecognized file in archive/", it did NOT delete it — that's the safety guard working.
- [ ] `[app]` **User replacements are never deleted.** Organize a mod, deactivate it (parked), then manually drop a *different* file with the same name into the flat `mods/` folder. Re-activate then deactivate (or Flatten): Silo must **leave your file untouched** and report "isn't the one Silo projected", never delete it.
- [ ] `[app]` **Scans stay fast + safe.** A full rescan of the large library completes without error (zip-read caps don't reject normal mods).

## 15. Full UI coverage sweep — open every panel, drawer, toggle, modal

The frontend was split into ~20 new components, so this section exists to **touch every
interactive surface at least once** and catch a wiring regression that compiles fine.
For each item: it **opens/toggles without error, renders its content, and closes cleanly**
(via its ✕, the backdrop, or Esc), and re-opening works. **Keep the devtools console open
(if available) and confirm no red errors appear** as you open/close each — a broken prop
or `$effect` in an extracted component shows up there first. None of these should cover the
top header bar (the drawer/backdrop must start below it).

**Top bar (always visible):**
- [ ] `[app]` Library ↔ Browse tabs switch; switching **closes an open detail drawer** (no overlap).
- [ ] `[app]` **Savegames** button opens the Savegames panel; closes.
- [ ] `[app]` **Loadouts** button opens the Loadouts panel; closes.
- [ ] `[app]` **Multiplayer** button opens the MP-sync panel; closes.
- [ ] `[net]` **Updates** (⟳) opens the Updates panel; closes.
- [ ] `[app]` **Rescan** runs a scan (spinner → counts refresh).
- [ ] `[app]` **Settings** (gear) opens; closes.
- [ ] `[game]` **Launch** is present (don't have to run it here).

**Library stat/filter bar:**
- [ ] `[app]` **mods** tile opens the library **Stats** panel; closes.
- [ ] `[app]` **conflicts** tile opens the **Conflicts** panel; closes.
- [ ] `[app]` **need attention** tile opens the **Health** panel; closes.
- [ ] `[app]` **◆ diagnose** opens Crash/Log triage; **⌨ bindings** opens the bindings map; **⛓ bridge** opens the bridge tool. Each closes.
- [ ] `[app]` Filter toggles each flip and re-filter the list: **Favorites**, **Hidden**, **⚑ Flagged**, **⚠ In conflict** (disabled when 0). Combine two, then clear.
- [ ] `[app]` **Search** box filters live; clearing restores the list.

**Library toolbar + rail:**
- [ ] `[app]` **Category rail**: click a category and a subcategory → list narrows; "All mods" resets.
- [ ] `[app]` **Right-click a category** → context menu appears and dismisses.
- [ ] `[app]` **Sort** dropdown cycles Name / Category / Size / Recently added / Version / **My rating**; the **↑/↓** direction button flips order.
- [ ] `[app]` **Select-all** checkbox activates/deactivates the filtered set (indeterminate state renders when partial).

**Library detail drawer (click a mod row) — every sub-section:**
- [ ] `[app]` Header (icon/title/author/version/tech name) + **✕ closes**; **drag the left edge to resize** (persists); backdrop click closes.
- [ ] `[app]` Action buttons all respond: **Active/Parked**, **Favorite**, **Broken**, **Hidden**, **Reveal**, and **Settings** (only when the mod has settings).
- [ ] `[app]` **Category editor**: change the dropdown/subcategory → **Save** enables; **Reset to auto** shows for an overridden mod.
- [ ] `[app]` **Last run** + **Catalog** status render (the `ModStatus` component).
- [ ] `[app]` **Rating / tags / notes** (the `ModCuration` component): set a star, add + remove a tag, type a note (saves on blur).
- [ ] `[net]` **GitHub link** (`ModRepoLink`): the owner/repo field, Check, and Install controls render.
- [ ] `[app]` **Dependencies**, **uniqueType**, and **Conflicts** sub-sections render when present.

**Settings panel — every control:**
- [ ] `[app]` **Theme** System/Light/Dark switch (both themes legible).
- [ ] `[app]` **GitHub account**: Connect / **Enable actions** / **Use a Personal Access Token** / Disconnect all render (see §14 for the real connect).
- [ ] `[app]` **Library layout**: **Organize**, **Rebuild categories**, **Restore vanilla**, and the **auto-file toggle** are present. Confirm auto-file is **OFF by default** on a fresh profile.

**Browse tab — every surface:**
- [ ] `[net]` **Sort** dropdown (Popular / Most downloaded / Top rated / Newest / Name) and **category** dropdown both re-query.
- [ ] `[net]` A **card** shows title/author/rating/downloads badges + per-source chips; **Details** opens the Browse drawer.
- [ ] `[net]` **Browse drawer**: facts, the full **Available from** source list, and the **Interact** cards render — **GitHub** (★/⑂/👁/◎ + Star/Watch), **ModHub** (⭐ + Rate ↗).
- [ ] `[net]` **Read more** opens the **description modal** (full body + "Open full mod page"); closes.

**Every remaining modal opens + closes cleanly:**
- [ ] `[app]` Crash triage · Guided bisection intro · Bindings map · Multiplayer sync · Filltype bridge · Savegame config editor · Per-mod settings form · Updates · Diagnostics "Export report".

## Sign-off
- [ ] All `[app]` items pass.
- [ ] All `[game]`/`[net]` items pass (or known-limitations noted below).
- [ ] `npm run tauri:build` produces a launchable packaged app.

**Known limitations to note at release:** ModHub are index-only (open-page, no direct
install — GIANTS gate downloads); ghost-keybind cleanup deferred (unverified premise);
override-conflict detection is fillTypes-only for now.
