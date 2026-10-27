# Changelog

All notable changes to Silo are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Silo's source code now lives at
  [github.com/HLLMR-VENTURES/silo](https://github.com/HLLMR-VENTURES/silo). Links in the app and
  docs point there, and the updater checks the new address first. The old `HLLMR/silo` address
  still redirects and stays as the updater's fallback, so existing installs keep updating.
  Downloads of 1.0.1 and earlier verify with `--owner HLLMR`; see
  [Verify your download](docs/VERIFY_DOWNLOAD.md).

### Fixed

- macOS: on Apple Silicon, the downloaded app no longer reports "Silo is damaged and can't be
  opened". The whole app bundle now gets a valid ad-hoc signature, so macOS shows its usual
  prompt for an unidentified developer instead (System Settings → Privacy & Security → Open
  Anyway). The release build now fails if that signature doesn't verify. Apple Developer ID
  signing is still tracked in #171. (#322)

### Security

- Updated the TLS certificate libraries (`rustls-webpki` 0.103.16, `webpki-roots` and
  `webpki-root-certs` 1.0.10) so Silo's HTTPS connections no longer trust the distrusted emSign
  intermediate certificates (RUSTSEC-2026-0381, -0382, -0383).

## [1.0.1] - 2026-10-20

### Fixed

- On Windows, typing in a text field no longer pops up the system's "Saved info" suggestions over
  Silo. (#303)

### Security

- Releases are signed only for a version tag on a commit that is already on `main` with every CI
  check green, and only after the owner approves the signing step; the signing keys are no longer
  available to any other workflow run. CI now also scans the whole history for committed secrets.

## [1.0.0] - 2026-10-19

Silo 1.0 is a redesign from end to end: a new layout, plainer words, and every page rebuilt
around what you do next. Your library, tags, notes and saved loadouts (now **mod sets**) carry
over. The library database moves to version 2 (one added column); Silo keeps a copy of the old
one in `db-snapshots/silo-before-v1.db` before upgrading. Silo 0.11 won't open a version 2
database; that copy is kept in case you need to go back.

### Added

- **A new layout.** A sidebar takes you to Library, Browse, Mod sets, Diagnose and Updates, with
  Tools, Settings and Help below. The header holds the search (your mods, or the catalog on
  Browse), a status chip that opens Diagnose, an activity bell, and Play with a menu for the game
  folder and your mod sets. A mod's details sit beside the list, so you can keep clicking through
  mods. Silo reopens the page you were on last, except Browse: a launch never contacts the
  catalog unasked. (#230)

- **Mod sets.** Loadouts, Collections, Multiplayer sync and Savegames are one page with **My
  sets**, **Open a shared set** and **Savegames**. Each set is compared with your active mods
  ("Same" or "+31 / −77"). Switching shows what changes, saves your current mods as an automatic
  "Before …" set first (the newest five are kept) and offers Undo. After you change mods, the
  Library and Play menu say "Custom (3 changes since Harvest 2026)" with **Update the set** and
  **Save as new**. (#226, #251)

- **One Share dialog.** Share a set, or your active mods, as an unlisted link on your GitHub, a
  `.silo` file, or a Multiplayer check file (`.silomp`). Links need GitHub; files need no
  account. **Open a shared set** takes a link or a file and shows a preview first: nothing
  downloads, saves or switches until you click. A Multiplayer check file is checked against your
  active mods, with Turn on, Park and Find in Browse on each mismatch. (#251)

- **Savegame cards.** Each save shows its map, when it was last played and how many of its mods
  you have; **Use this save's mods** goes through the same switch dialog. (#251)

- **Crash notice.** When your last game ended unexpectedly, Silo says so as soon as you come back
  and names the most likely mod with **Park it**, or offers **Find the problem mod** when the log
  names none. (#260)

- **Diagnose.** **Last run** ranks the likely causes with the first log line and **Park**, **Show
  in library** and **Copy details** on each mod. **Conflicts** leads with "0 problems · 12 to
  review", labels each one Problem, Review or Note in plain words, and adds **Park one…**.
  **Problems** (formerly library health) has an action on every row. (#260)

- **Find the problem mod**, the guided search formerly called bisection, shows "Round 2 of about
  8", keeps running while you use the rest of Silo, and ends with **Put my mods back and park
  it**. (#230, #260)

- **Updates, in sections:** Ready to install, Download from ModHub, Check these yourself,
  Couldn't check (with the reason and Try again), and collapsed Up to date and Not tracked lists.
  **Update all** installs one at a time with progress and one summary at the end, and stops if
  Farming Simulator 25 is running. ModHub mods: **Open on ModHub**, save the zip into your mods
  folder, then **Use this version**. An optional check when Silo starts is in Settings → General
  (off by default; it only checks). (#244)

- **Library.** Filters you can see (All / Active / Parked, Favorites, Needs attention, and more
  under **Filters**, each shown as a removable chip). Select several mods with checkboxes,
  Shift+click or Ctrl+click and turn them on, park, favorite or hide them together. Turning mods on
  or off offers **Undo** for 10 seconds. List view is the default, with a Comfortable or Compact
  density in Settings → General. (#242)

- **Browse.** Sources spelled out with their own versions ("GitHub 1.0.6.0", "ModHub 1.0.5.0"),
  one button per card that follows the mod (Install, progress, Show in library, See update, Open
  on ModHub), a **Verified build** badge for catalog-hashed builds, and clear loading, offline
  and no-results states. (#246)

- **Mod details, reordered.** Verdicts first (Verified build or Modified, Update available,
  Problem), then the Active switch, Settings and a ⋯ menu, then whatever needs your attention.
  (#245)

- **First run.** A new library opens a short setup that shows the mods folder Silo found,
  explains active and parked mods, and shows exactly what organizing would do before anything
  moves. **Skip setup** moves nothing. Upgrading users get a one-time **What's new in Silo 1.0**
  card instead. (#250)

- **Help.** Searchable help topics (an old name like "loadouts" finds Mod sets), What's new,
  Keyboard shortcuts, **Report a problem** (copies Silo's diagnostics and opens the GitHub bug
  form), and About with Silo's license and the third-party licenses. (#230, #250)

- **Keyboard shortcuts.** Ctrl+K or / searches, F5 or Ctrl+R rescans, Alt+1 to Alt+5 switch
  pages, Ctrl+, opens Settings, F6 moves between regions; press ? for the full list. The Library
  list and Browse grid are one Tab stop each, with arrow keys inside. (#230, #242, #246)

- **An activity list.** The bell lists what Silo is doing now and what it said recently, each
  error with **Copy details**. (#228)

- **UI size** (90% to 125%) in Settings → General, Silo's own fonts bundled with the app, and
  the silo mark at the top of the sidebar. (#224)

- **Open log folder** in Settings → Diagnostics. (#238)

### Changed

- **Fewer false conflicts.** More than one active map is a Note (only the map your savegame uses
  loads), and two mods using the same vehicle, building or tool type name are no longer a
  conflict: the game files each mod's types under the mod's own name, so both load. (#260, #275)

- **"Loadouts" and "Collections" are now "mod sets" and "shared sets"**, and guided bisection
  is **Find the problem mod**, everywhere in the app. A `silo://collection` link and a
  `silo.hllmr.com/c/…` link both open in Mod sets → Open a shared set. (#251)

- **Settings is a page with tabs:** General, Mods folder, Library, Accounts, Diagnostics,
  Maintenance and About. Renamed: **Rebuild categories** is **Re-read every mod**, **Optimize
  links** is **Reclaim disk space**, and **Restore vanilla layout** is **Undo organizing**
  (Settings → Maintenance). (#230, #236)

- **Tools** holds Library stats, the Controls map, the Fill type bridge and Graphics settings
  (game.xml). (#230)

- **The Organize dialog** shows a summary first, every file behind **See every file**, progress
  while filing, and any mod that couldn't be moved with **Copy details**. **File new mods
  automatically from now on** replaces "Don't preview next time". (#250)

- **Plain-English messages.** Errors say what happened and what to do, with Try again where it
  helps and the technical text behind **Copy details**. Confirmations open inside Silo and name
  the action ("Delete", "Switch"). Warnings share one notice strip under the header instead of
  stacked banners. Buttons that are off say why. (#226, #228, #238, #250)

- **More than one active map is a Note, not a critical conflict.** An in-game test showed
  Farming Simulator 25 loads only the map your savegame uses. (#260)

- **Easier to read.** Higher contrast in both themes (including for colour-blind users), nothing
  smaller than 12 px, a visible focus outline, one consistent icon set, and dialogs that work
  with a keyboard and screen readers. (#224, #258)

- **Wider Library and Browse on large screens** (up to 1920px). (#238)

- **Ctrl+R and F5 rescan instead of reloading the window,** which dropped whatever Silo was
  doing. (#230)

### Fixed

- Checking for updates works again on libraries with more than 500 mods, and a refused check no
  longer claims you're offline. (#252)
- After switching mod sets, the status chip and Conflicts could show the previous set's result.
  (#260)
- Saving a set under a name you already use offers to replace it (asking first, with what
  changes) instead of creating a duplicate; using a savegame's mods switches to exactly the mods
  it showed you. (#226)
- When some mods couldn't be filed, the ones that were filed stay active, and the filing error
  stays on screen after the rescan. (#222, #250)
- A rescan no longer hides a warning that's still true. (#228)
- Sorting the Library by version is much faster on big libraries. (#258)
- Faster start when the mods folder can't be read. (#222)
- Tooltips no longer linger after their button disappears; flagged mods are "Flagged"
  everywhere. (#238)
- Browse no longer leaks a listener on every visit. (#246)
- **Find the problem mod parks every mod it tests.** Mods you hadn't filed into the library yet
  were never actually parked, so the newest mod (usually the culprit) could never be found. Silo
  now offers to file them first (they stay active), keeps unzipped folder mods on every round,
  stops a round if a mod it meant to park is still in the mods folder, checks that the game has
  really closed before asking how the round went, and keeps the round when a launch or a step
  fails. Stopping asks first. The Library and Play show the round's real mods. (#283, #291)
- **Number fields in the settings editors save again.** Editing a number in Graphics settings,
  a savegame's settings or a mod's settings failed on save. A failed save also keeps your edits,
  and Performance class shows the game's own value. (#278)
- **A mod's details stay with that mod.** An integrity verdict, a GitHub update check or a note
  you were typing could land on the next mod you clicked; a check for one mod could offer to
  install onto another. The details also follow the mod after a rescan, filing or category
  change. (#270, #276, #297)
- A mod's title or author with an `&` in it was cut short ("Elm & Ash" showed as "Elm"), and a
  savegame or map name with one lost its first half. Silo reads the whole name, and re-reads mods
  it had already scanned once after the update. (#266, #268)
- Organizing, parking or flattening a mod the game (or an antivirus or sync client) had open could
  leave two copies of it, or a half-deleted mod folder. Silo copies only when the mod is going to
  another drive, and takes its copy back out if the original can't be removed. (#279)
- Installing from Browse or a shared set stopped at 500 MB; every download now allows up to
  4 GB. Previewing a shared set while the catalog couldn't be reached no longer lists every mod as
  "not in the catalog". The Multiplayer check reads mods in small pieces instead of whole files.
  (#282)
- **Updates.** A failed first check says so (with Try again) instead of starting over; a release
  you already have under another spelling ("v1.2.0" vs "1.2.0.0") or an older one is no longer
  offered; a GitHub release with no zip opens its page; results from another mods folder are
  cleared when you switch. Versions show the same way everywhere ("1.6.4.0", never "V.1.6.4.0").
  (#273, #294)
- An earlier Silo's user who never organized but had sets, tags or notes gets What's new, not the
  first-run setup. (#289)
- The Library's In conflict badge and filter match the right mods; "Needs update" no longer says
  everything is current when Silo couldn't check; switching a mod during a scan no longer bounces
  back. (#284, #297)
- Smaller fixes: each page opens at the top; Ctrl+A selects every shown mod; no stray
  horizontal scrollbar in dialogs; a cleared notice no longer turns the bell red; the Organize
  dialog can't get stuck when re-linking fails; a duplicate fill type no longer crashes the
  bridge tool; Save raw XML refuses text that isn't valid XML; a mod with no version shows
  Unverified; an unfinished bridge write leaves nothing behind; "No game log yet" is a calm empty
  state, not an error. (#271, #280, #285, #292, #293, #295, #296, #299, #300)
- Plainer words across the app: the 1.0 names everywhere, real plurals, one Check now, and error
  messages that say what to do. (#286, #287, #288, #290, #298)

### Security

- **Shared sets have a size limit.** A shared set (a link, gist, repo or `.silo` file) may list up
  to 5,000 mods, and a shared file may be up to 8 MB, so a hostile link can't make Silo send
  hundreds of catalog requests. Catalog lookups also skip blank and repeated names. (#277)
- **A mistyped GitHub token can't end up in the log.** A pasted token with a line break or an
  invisible character is refused before it's sent, and anything shaped like a GitHub token or a
  `Bearer` credential is masked in the log and in diagnostics. Disconnecting GitHub no longer
  leaves an old fallback token readable in Silo's database file. (#281, #293)
- Silo's outbound-address check also refuses IPv4 addresses written as IPv6, `0.x.x.x`, and
  other spellings of localhost. (#282)

## [0.11.0] - 2026-10-17

### Added

- **Automatic database backups.** Silo keeps a copy of its library database (tags, notes,
  ratings, loadouts, settings) from each of the last five days you used it, in its app data
  folder, plus one copy taken just before an upgrade changes the database. Backups never
  include your GitHub token. (#216)

- **Silo keeps a log.** Silo writes a log file in its app data folder (five files of 2 MB at most),
  so problems can be traced after the fact. Every file Silo moves, links, replaces, backs up or
  edits is recorded. (#217)

- **Copy diagnostics.** Settings → Diagnostics → Copy diagnostics copies your report and Silo's
  recent log, with your user name removed, ready to paste into a bug report. If the clipboard can't
  be used, Silo offers to save them to a file instead. Exported reports leave out your user name
  too. (#217)

### Fixed

- **Your original game settings are kept.** The first time Silo edits gameSettings.xml or a mod's
  settings file, it keeps the untouched original (or the oldest backup an earlier Silo left)
  beside it (`.silo-original.bak`), plus the three versions before each edit (`.bak`, `.bak.2`,
  `.bak.3`). Before, every edit replaced the only backup. (#213)

- **Mod icons stay cached across Silo updates.** The icon cache uses stable names, so icons
  aren't decoded again after every update. (#213)

- **Settings edits aren't lost to the game anymore.** Silo won't change gameSettings.xml or a
  mod's settings while Farming Simulator 25 is running, because the game writes its own copy
  back when it exits. Close the game first. (#214)

- **Silo stays responsive while it reads and saves.** Backing up a savegame, opening or saving
  settings, importing or exporting loadouts and similar actions now run in the background
  instead of freezing the window. (#214)

- **The open-folder links in Settings work.** They silently did nothing before. (#215)

- **Going back to an older Silo can't damage your library.** Silo now records its database
  version and won't touch a database made by a newer Silo; it asks you to install the latest
  version instead. (#216)

- **A damaged database no longer breaks Silo.** If Silo's database can't be read, Silo keeps the
  damaged file, restores its latest backup copy (or starts fresh), and tells you what happened.
  Your mods are never touched. (#216)

- **Silo can find a library archive on another drive again.** A custom archive now holds a small
  `.silo-archive.json` note and your mods folder points to it, so Silo finds the archive even if
  its database is lost. (#216)

- **Rescanning never fights a change in progress.** A rescan, for example when you switch back
  to Silo, no longer rewrites Silo's record of your library while another change is running.
  (#218)

- **A mods folder can't share files with an archive.** Silo refuses a mods folder that is another
  mods folder's archive, sits inside one (a category folder), holds one, or whose own archive
  would be one. The folder Farming Simulator's own setting points at counts too: if it shares files
  with an archive, Silo says why and won't change files there until you choose another folder (or,
  when only that setting is the problem, point Farming Simulator at your folder).
  Before, two layouts could share files and organizing one moved the other's mods. (#220)

- **Optimize links handles an archive on a network share.** On a drive Silo can't identify, it
  now tests once whether a hardlink works instead of assuming, so Optimize links no longer tries a
  hardlink for each mod and fails, and the move dialog says the same as what the move does.
  (#220)

- **Cancelling an archive move leaves no empty folders behind.** Category folders Silo made in
  the new location are removed when they're empty, also for mods whose copy was stopped partway.
  Folders that were already there, or that hold anything, are kept. (#220)

### Security

- **Stricter file-name checks.** File names with a colon, a trailing dot or space, or a reserved
  Windows device name (such as `CON` or `NUL`) are refused. (#213)

- **Savegame backups only copy real savegames.** Silo now checks the folder against the
  savegames it found before copying anything. (#214)

- **The catalog address is fixed in release builds.** Silo always downloads catalog mods
  through its own catalog; the developer-only setting to change it no longer exists in the
  release app. (#214)

- **Silo decides where it writes.** Changing the mods folder or the library archive now uses a
  folder picker Silo opens itself, and Silo only switches to a folder chosen there or one it
  already knows. Editing game settings can't change FS25's mods folder either. (#215)

- **Updates only replace mods in your library.** An update can only swap a mod's zip in your
  mods folder or library archive, never any other file there. (#215)

- **Collections install exactly what the preview showed.** Silo matches every mod the same way
  in the preview and the install, and won't install if the collection, or what the catalog
  found for it, changed after you previewed it. (#215)

## [0.10.0] - 2026-10-17

### Added

- **Remember where a mod came from.** In a mod's details, save the page you downloaded it from.
  Silo keeps the link, opens it in your browser, and lists these mods under *Check these
  yourself* when you check for updates, along with when you last looked. Thanks to @iwatkot for
  the idea and design input (#196).

### Changed

- **Silo no longer sends your mod list in the background.** Every scan used to send the names
  of all installed mods to the catalog, only to fill the region / era / theme / brand / realism
  dropdowns in the Library. Those dropdowns are gone from the Library (they stay in Browse),
  and Silo now contacts the catalog only when you ask: searching Browse, opening a mod's
  details, checking for updates, or checking a mod's integrity.

- **Silo no longer works around ModHub's image protection.** GIANTS' ModHub CDN only serves
  images to its own website. Silo used to send a farming-simulator.com `Referer` header to get
  thumbnails anyway; it now sends its own User-Agent and no Referer, and only proxies images
  hosted on GitHub. ModHub thumbnails come from Silo's catalog cache as before; the occasional
  brand-new mod that isn't cached yet shows a placeholder until the next catalog refresh.

- **New look for the website and README.** "More farming. Less fixing." Copy now describes
  crash triage as ranking the likely culprit, and drops claims we couldn't back up.

- **Honest copy everywhere.** The website, help and README now describe exactly what Silo does, the privacy policy lists every connection Silo makes, and the arbitrary "trust score" is gone.

### Fixed

- **Folder mods: your edits are safe.** On Windows, folder mods are now linked as NTFS
  junctions instead of copied, so edits you make in the mods folder land in your library. Where
  Silo still has to copy a folder (some drives and cloud folders), parking or restoring the
  vanilla layout only deletes the copy if it's unchanged; otherwise it's moved to `backups/`
  and Silo tells you where. (#202)

- **A stalled download can't freeze Silo anymore.** Network requests now time out instead of
  waiting forever, and mods download before Silo locks your library, so other changes aren't
  blocked while a download runs. (#202)

- **Guided bisection puts back exactly what you had.** Finishing a bisection now restores the
  set saved when it started, even if Silo rescanned while you were testing in the game, and any
  problems are shown instead of ignored. (#202)

- **Mod updates you can undo.** Updating a mod no longer overwrites it in place: Silo downloads
  the new version beside it, swaps it in, and keeps the previous version in `backups/` (the
  newest three per mod). If Silo stops mid-update, it finishes or rolls back the swap on the next
  scan and tells you what it did. Mods aren't updated while Farming Simulator 25 is running.
  Silo also asks before quitting, and won't install its own update, while it's still changing
  files. (#202)

### Security

- **Stricter image-host check.** The image proxy now parses URLs properly before checking the
  host, so look-alike or malformed URLs (for example one using a backslash to disguise its real
  host) are refused.

- **Stricter checks on GitHub requests.** Repository names and gist IDs are validated before
  Silo uses them, GitHub API addresses are built safely, and mod updates download only from
  GitHub. (#202)

## [0.9.0] - 2026-10-17

### Added

- **Put your library archive on any drive.** Settings → Library archive → **Change…** moves your
  parked mods (for example to a big HDD) while your mods folder stays where the game reads it. The
  move verifies every mod before removing the original, shows progress, refuses to run while
  Farming Simulator 25 is open, and can be **paused** — play, then resume — or **cancelled**, which
  moves everything back. Pointing it at a folder that already holds mods offers to file them into
  your library. Across drives, active mods are linked (Linux, macOS, Windows with Developer Mode)
  or copied. If the archive's drive is unplugged, Silo keeps your library and waits for it to come
  back — or, if you moved or restored the archive yourself, **Choose another** points Silo at it
  without moving anything. (#166)

### Fixed

- **Linux & macOS: folder mods linked with a symlink can be deactivated again.** Silo removed the
  link with a call Unix refuses for symlinks, so parking, restoring the vanilla layout, or moving
  such a mod failed. Symlinks are now unlinked properly; Windows was not affected. (#185)

### Security

- **Updated the zip library (2.4 → 9.0)** for RUSTSEC-2026-0367, -0368 and -0369, which concern
  extracting archives with hostile symlinks or permission bits. Silo only *reads* mod archives and
  never extracts them, so it wasn't affected in practice; scanning, conflict detection and integrity
  hashes are unchanged (verified against the full test suite and a real 737-mod library). The
  auto-updater still bundles an older zip with no fixed release yet; that path is unreachable
  because updates are signature-checked before anything is unpacked, and it's tracked to remove. (#182)

## [0.8.0] - 2026-10-16

### Added

- **Choose your mods folder.** Settings → Mods folder → **Change…** picks any folder. If FS25 reads
  a different one, Silo offers to point the game at it too, keeping a backup of `gameSettings.xml`.
  Silo also follows FS25's own *mods directory override* automatically, and remembers each folder's
  organized library separately, so switching back restores it. (#162)
- **Linux (Steam Proton) and Mac App Store detection.** Silo now finds the FS25 user folder inside
  the Proton prefix in any Steam library (native or Flatpak), and in the sandboxed Mac App Store
  container. (#162, #171)

### Fixed

- **"Cancel" now actually cancels.** Every confirmation dialog ignored Cancel and went ahead anyway,
  including *Restore vanilla*, deleting or updating a shared collection, deleting a loadout,
  activating/deactivating the whole library, launching with critical conflicts, and installing a
  GitHub update. The dialog plugin makes the browser's `confirm` asynchronous, and Silo wasn't
  waiting for the answer. All confirmations now wait for your choice, and the build fails if an
  unawaited one is ever added again. (#177)
- **macOS: the game is now detected as a `.app` bundle.** Silo looked for a bare
  `FarmingSimulator2025` file, but Mac games are app bundles (folders), so detection failed and the
  Launch button stayed hidden. Silo now finds the bundle in your Steam library, in `/Applications`
  or `~/Applications`, and falls back to Spotlight (bundle id `com.giants-software.fs25`). (#171)
- **macOS: Launch works for Mac App Store and GIANTS-direct installs.** Steam installs still launch
  through Steam; other installs open the game's app bundle directly. Guided bisection uses the same
  launch. (#171)
- **macOS / Linux: folder mods now project onto drives that refuse symlinks** (exFAT, FAT). Silo
  falls back to a marked copy, as it already did on Windows. On every OS, the copy fallback no
  longer runs when a folder with that name already exists, so it can't merge into your own folder.
  (#171)

### Changed

- **CI now builds and tests on macOS**, so macOS-only code is checked before a release. (#171)
- **Updated Tauri to 2.12 and its plugins** (dialog, opener, process, deep-link, single-instance,
  updater), with matching frontend packages. (#165, #169)

### Security

- **Updated the TLS library (`rustls` 0.23.42 → 0.23.46)** to fix RUSTSEC-2026-0285, where TLS 1.3
  handshake messages could be accepted across encryption-level boundaries. Silo uses it for its
  HTTPS connections (catalog, GitHub, updates). No behavior change. (#167)

## [0.7.3] - 2026-08-12

### Removed

- **Nexus Mods integration has been removed.** Silo's catalog now aggregates **ModHub and GitHub**
  only. After a policy review by Nexus Mods, we removed the integration rather than change how the
  catalog works: the app's Browse and per-source UI, the SiloAPI catalog ingest, the website, and
  the docs. No Silo feature depended on it.

## [0.7.2] - 2026-08-11

### Added

- **Adopt a dropped-in update right from the mod's drawer.** When you update a mod the ModHub
  way — download the new zip into your mods folder over Silo's managed copy — the drawer now shows
  a clear banner (“A newer build is in your mods folder — vX … Adopt it”) with **Adopt vX** and
  **Keep Silo's vY** buttons. Adopting promotes the dropped build into the library (old copy kept
  in `backups/`) and updates the version. Previously this only surfaced as a red “foreign file”
  error and an entry buried in the health panel.
- **"Find in Library" from Browse** — the reverse of the library's "Find in Browse". An installed
  mod's Browse card and detail drawer now offer a jump straight to that mod in your Library
  (filtered to it, drawer open).
- **Clickable mods behind an open drawer.** The detail drawers are now non-modal: with a drawer
  open you can click another mod in the list behind it to switch the drawer to that mod, instead
  of the click just closing it. Close with the ✕ or the Escape key.

### Fixed

- **Deactivating a loose (unorganized) mod now works.** A mod sitting loose in the mods folder
  (e.g. one you just downloaded) isn't a Silo projection, so turning its toggle off did nothing —
  the file stayed and the toggle sprang back on the next scan. Parking a loose mod now files it
  into the library (moves it into `archive/`, unprojected), so it leaves the live folder and stays
  available. Reversible — toggling it back on re-projects it.
- **"Find in Browse" now opens the mod.** It switched to Browse and filtered to the mod but left
  it unselected; it now opens that mod's detail drawer once the results load.

### Changed

- **The header's second row (Savegames · Loadouts · Collections · Multiplayer) is now centered.**
- **The header bar now has an intentional two-row layout** instead of wrapping ad hoc as the
  window narrows. The view tabs and app actions (Updates, Rescan, Launch, Settings) stay on the
  main row; the panel toggles (Savegames, Loadouts, Collections, Multiplayer) sit on a dedicated
  second row.

## [0.7.1] - 2026-08-11

### Changed

- **Deactivating mods is no longer slow on cloud-synced libraries.** Removing a projected file
  used to content-hash it first to prove it was Silo's own — gigabytes of hashing on a copy-based
  library. Silo now records each projection's identity (size + modified-time) when it writes it,
  so a deactivate confirms ownership instantly and skips the hash. Still safe: a file you swapped
  in has a different modified-time, so it falls back to the careful check and is never deleted.
  Foreign-file scans warm this record, so the one-time hash happens once in the background, never
  again.
- **Toggling the entire library at once now asks first.** Select-all across a *filtered* view is
  unchanged, but sweeping the whole unfiltered library (25+ mods) confirms first — on a cloud
  folder that's a real gigabytes-of-files operation, easy to trigger by accident.

### Added

- **Cloud-synced mods folders are now detected and handled.** OneDrive, Google Drive, and
  Proton Drive present `Documents` as on-demand placeholders that reject hardlinks, so Silo
  projects the active set as copies there. Silo now probes for hardlink support, shows a
  one-time notice explaining the trade-off (more disk, slower bulk changes; exclude the folder
  from cloud sync for best performance), and **“Optimize links” recognizes the situation** and
  says so instead of throwing an error per mod.

### Fixed

- **Foreign-file detection no longer freezes the window.** The `detect_foreign_files` command
  was synchronous, so it ran on the UI thread — and to tell a swapped-in file from Silo's own
  copy it content-hashes anything that isn't a provable hardlink. On a library projected as
  copies that's gigabytes of hashing on the main thread, every scan, which showed as
  "Not Responding" on launch. It now runs off-thread. (Running **Optimize links** removes the
  hashing entirely, since hardlinks are identified instantly.)
- **The update check no longer runs automatically on launch.** On a large library the burst
  of scan + catalog-tags + a whole-library update pass all completing at once, right as the
  window opened, could make it briefly stop responding. The **“Needs update” filter now checks
  on demand** — it's always clickable, and the first time you switch it on it runs a quiet
  check and then filters (or tells you everything's current). Nothing surprising happens on open.
- **Activating/deactivating a large library no longer looks like a crash.** A mod that was
  projected into the mods folder as a full **copy** (instead of a hardlink) forced every
  later activate/deactivate to `sha256` the whole file — on a big library that's gigabytes of
  hashing with no progress bar, which read as a frozen app. The active-set now reconciles
  against disk after a bulk change so the view can't drift out of sync with the folder.

### Added

- **Settings → “Optimize links (reclaim space)”** — collapses any active mod that was projected
  as a byte-for-byte copy back into a hardlink to its archived copy. Reclaims the duplicated
  space and makes future activate/deactivate instant. Progress-reported, and safe by
  construction: it only touches a file byte-identical to the archived copy, never a build you
  swapped in yourself.

## [0.7.0] - 2026-08-10

### Added

- **A landscape tile view for the Library** (now the default; toggle back to the compact list
  from the toolbar). Two widescreen tiles per row: a square icon, the title / author / version /
  size / verified / update flag with favorite-hidden-flagged actions, and a pill footer for the
  category and type. Click anywhere on a tile to open its drawer (no more Details button), it
  lifts on hover, and an **active mod gets a bright-green border and a green on/off switch** in
  its header that parks or activates it.
- **Silo detects which mods have settings by looking inside them** — not just the ones that
  have already been run. It scans each mod for a shipped settings XML or Lua that persists to
  `modSettings/`, lifting detection from ~1 mod (only what had a runtime folder) to well over a
  hundred. Two new Library filters use it: **⚙ Has settings** and **⬆ Needs update** (next to
  Favorites / Flagged / In-conflict). The detail drawer badges a detected-but-not-yet-editable
  mod as "has settings — launch once to configure."
- **The Library's filter bar has the same facet dropdowns as Browse** — **region · era · theme
  · brand · realism** — built from your installed mods' catalog tags (fetched in one batch on
  scan) and filtered entirely client-side.

### Changed

- **One unified search/filter bar across Browse and the Library.** Category (a parent +
  subcategory pair, whose "all of a parent" option now works catalog-side), search, sort (with a
  working ascending/descending toggle and a "recently added" default), and the semantic facets
  all live in one shared two-line bar on both screens. On the Library it adds a second row for the
  library-only filters (Favorites · Hidden · Flagged · In-conflict · Has-settings · Needs-update);
  the old left category rail is gone, the list matches Browse's content width, and the stats +
  tools (diagnose / bindings / bridge) now sit in a content-width footer that floats over the
  scrolling list.
- **Browse caches each view, so going back to one is instant.** Revisiting a filter/sort combo
  you've already loaded no longer re-polls the catalog — results (and everything you'd paged in)
  come straight from an in-session cache.
## [0.6.0] - 2026-08-06

### Fixed

- **Mod settings forms now recognize the `*Value` attribute convention** (`boolValue`, `intValue`,
  `floatValue`, `stringValue`) that real mods like Easy Dev Controls use — previously only
  `boolean`/`integer`/… were parsed, so those mods' settings came up as an empty form. Metadata
  attrs (e.g. `isSaved`) are preserved verbatim on save.

### Added

- **Filter Browse by facets.** A row of dropdowns in Browse scopes the catalog by **brand,
  theme, region, realism, and era** (from SiloAPI's semantic tags — pick one per facet and they
  combine), plus an **"available by year"** filter for period-correct playthroughs (only machines
  that existed by that year). A mod's tags also show in its detail drawer.
- **The library detail drawer now shows catalog info** — a mod's summary (clamped, with **Read
  more** for the full text), where it's **available** (ModHub / GitHub, with links), and
  whether it's **outdated** (⬆ update available vs. ✓ up to date). No more hopping to Browse to
  find out if a library mod has a newer version.
- **An "updates" filter in the stat bar.** After running ⟳ Updates, a gold **N updates** chip
  appears when any library mods are outdated; click it to scope the list to just those. (Pairs
  with the drawer showing per-mod update status.)
- **The library auto-refreshes when you return to Silo.** Download a mod in your browser into
  the mods folder, alt-tab back, and it appears — no manual Rescan or restart. Triggered on window
  focus, throttled so quick tabbing doesn't re-scan repeatedly (the scan cache keeps it cheap).
- **Click a stat to filter the library.** The **maps**, **script mods**, **uniqueType**, and
  **active** counts in the stat bar are now toggle filters — click one to scope the list to it,
  click again (or another) to switch or clear. "active" gives you the quick "show only what's
  loaded" view.

## [0.5.0] - 2026-08-04

### Fixed

- **"Newest" in Browse now matches ModHub's order.** It was sorting by our catalog's
  refresh timestamp (so it barely changed anything); it now uses SiloAPI's `newest` sort,
  backed by ModHub's native grid ordinal — the same order you see on ModHub, updates and
  re-releases included. The web `/browse` "Newest" option uses it too.

### Added

- **Right-click menu on library mods.** Right-clicking a mod opens a context menu with its
  real commands — Activate/Park, Open details, **Find in Browse** (jumps to the catalog and
  searches for it), Change category, Edit settings, Favorite, Hide, Mark broken. The
  WebView's built-in reload/inspect menu is suppressed app-wide (cut/copy/paste still works
  in text fields).

### Changed

- **Collections is now its own toolbar entry**, split out of the Multiplayer panel.
  Sharing a curated set of mods as a link (and importing one) is a distinct feature from
  matching your active set to a host for a session, so it now lives under its own
  **Collections** button. `silo://collection` deep links open it directly.
- **A shared collection link is now a real page, not a raw gist.** Creating a collection
  gives you a `silo.hllmr.com/c/…` link. Recipients land on a page that shows the mod list
  and an **Open in Silo** button (with a download link if they don't have Silo yet) — instead
  of a wall of GitHub JSON. The gist/repo still backs it and gets a README pointing at the
  page. (Previously the "Open in Silo" button lived in the GitHub README, where GitHub's
  sanitizer strips `silo://` links, so it never worked.)
- **Manage your collections in-app.** The Collections panel now lists the collections you've
  published to your GitHub — copy a share link again, open its page, jump to it on GitHub,
  **Update** it (re-pin to your current active set, keeping the same share link), or delete it
  (secret gists delete in-app; public repos link out to GitHub, which Silo can't delete without
  extra permissions). No more digging through GitHub by hand.

## [0.4.0] - 2026-08-03

### Added

- **"Open in Silo" links for shared collections.** A public collection's page now has an
  **Open in Silo** button (a `silo://` link); clicking it opens the app straight to that
  collection's import — no copy-pasting the URL. Silo registers the `silo://` handler when you
  install it, and a second launch focuses the running window instead of opening a duplicate. (#72)

- **Release artifacts now carry a GitHub build-provenance attestation.** You can verify any
  download came from Silo's public build pipeline — `gh attestation verify <file> --repo HLLMR/silo`.
  See [docs/VERIFY_DOWNLOAD.md](docs/VERIFY_DOWNLOAD.md) for verifying the signature, checksum, and
  provenance of a download. (#60)

- **Every release includes a Software Bill of Materials** (`silo-sbom.cdx.json`, CycloneDX) — a
  full inventory of the Rust crates and npm packages (with licenses) that make up the build, so
  anyone can audit exactly what's inside. (#60)

## [0.3.1] - 2026-08-03

### Added

- **Windows installers are now code-signed** (Azure Trusted Signing). The `.exe`/`.msi` carry a
  verified publisher, so Windows SmartScreen no longer flags them as from an "unknown publisher."
  macOS/Linux builds remain OS-unsigned for now. (#60)

- **Dev mods are marked with a "Dev" badge in the library.** Unpacked folder mods (a development
  build you're working on, not a packaged `.zip`) now carry a badge so they're easy to spot and
  clearly distinct from managed library mods. Pairs with the earlier fix that made symlinked dev
  mods show up at all. (#76)

### Fixed

- **Release downloads now include a `SHA256SUMS.txt`** so you can verify your installer. The job
  that generates it used the wrong GitHub API endpoint and had silently never produced the file;
  it now attaches on each release. (#82)

## [0.3.0] - 2026-08-02

### Added

- **Collections — share a mod set as a link.** From Multiplayer sync, "Share as a link" publishes
  your set to your own GitHub — a **secret gist** for private/group sharing, or a **public,
  forkable repo** (with a generated README) for sharing widely — and hands you a URL (no mod files
  are uploaded, only the list — each mod pinned to a version and a content hash). Anyone you send it
  to can paste that link — gist *or* `github.com/owner/repo` — into "Open a shared link" to preview
  exactly what they already have, what's a different version,
  what Silo can install for them, and what they'll need to grab from ModHub — plus a heads-up
  for any dependency gaps or mod conflicts among the mods they already have — then import it:
  Silo downloads the installable mods, verifies each against the shared build (verified / modified),
  and saves the whole set as a loadout to apply. Sharing needs a one-time "Enable collection sharing"
  (the GitHub `gist` permission) in Settings; a secret gist is unlisted, not password-protected.
  (#61)

- **Library health flags foreign files in your mods folder.** If a file sits in the flat `mods/`
  root at a name Silo manages but isn't the projection Silo created (a build you swapped in, a
  leftover from another tool), Library health now surfaces it — so you know the mod that loads
  from that name may not be the one you expect. Silo still never touches it. Detected by
  identity (hardlink / symlink / marker / content), reusing the same check that guards deletions.
  (#34)

### Fixed

- **Activating an archived mod now works even if Silo's database drifted from disk.** If a mod
  sat in `mods/archive/` but wasn't in Silo's internal manifest (e.g. after the database was
  reset while the archive folder persisted), selecting it as active did nothing and it never
  loaded — with no error shown. Silo now reconciles the manifest from what's physically in
  `archive/` on every scan, so every archived mod is activatable. (#75)

- **Dev mods symlinked into your mods folder now show up.** A mod added as a symlink — a common
  developer setup, e.g. linking a mod's source folder into `mods/` — was skipped by the library
  scan, because a symlink's own file type is neither a plain file nor a folder. Silo now follows
  the link to see what it points at, so symlinked dev mods (and Silo's own folder-mod projections)
  are scanned like any other. (#76)

- **A savegame created while playing now appears on Rescan** instead of only after restarting
  Silo. The savegame list was read once at startup; Rescan now refreshes it alongside the mod
  library. (#77)

## [0.2.4] - 2026-08-01

### Security

- **Direct GitHub updates now confirm the download is the right mod before replacing it.** A GitHub
  release can hold many assets (source archives, docs, other editions), and a valid ZIP of the
  wrong mod is still a valid ZIP. Before overwriting an installed mod, Silo now requires the
  downloaded archive to contain a readable root `modDesc.xml` and — when the current file is a
  readable mod — to identify as the *same* mod (matching `uniqueType`, or author + title). A
  provable mismatch is refused, with the original untouched; missing/uncomparable fields never
  block a legitimate update. Mirrors the identity check catalog installs already do. (#20)

## [0.2.3] - 2026-08-01

### Security

- **Warn when credential storage isn't encrypted.** On a machine with no OS keychain, connecting
  a GitHub account falls back to storing the token in the local database. Settings now
  probes keychain availability and shows a clear warning above the account sections so it's never
  a silent fallback — the user decides before connecting. (#20)

- **SSRF guard on native requests.** A custom SiloAPI base URL and every mod download are now
  validated before the native layer fetches them: HTTPS only (plain `http` allowed only to
  localhost, for a self-hosted/dev endpoint), and never a loopback/link-local/private/CGNAT
  address. Blocks a compromised webview or catalog from steering a request at cloud metadata
  (169.254.169.254) or an internal service. Catalog images were already host-allowlisted. (#20)

- **Hardened the native command boundary against a compromised webview.** File-path commands
  that took a raw path from the frontend now validate it: loadout/mod-set/report export and
  import enforce the expected file type and reject `..` traversal, and config/mod-settings reads
  are confined to the FS25 user directory (mirroring the existing guarded writes). Turns
  "read/write any path" into "read/write an expected file type in an intended location." No
  behavior change for normal dialog-driven use. (#20)

## [0.2.2] - 2026-08-01

### Added

- **In-app auto-update.** Silo checks GitHub Releases on launch for a newer signed build and,
  when one is available, shows an **Update to vX** button in the top bar — one click downloads,
  verifies the signature, installs, and relaunches. Update artifacts are cryptographically
  signed in CI (minisign); the check fails closed and silently if there's no release, no
  network, or you're on a dev build. Auto-update takes effect from this build forward — install
  0.2.2 once, and future updates are one click.

## [0.2.1] - 2026-08-01

### Added

- **Version shown in the UI.** The app version now appears beside the **Silo** wordmark in the
  top bar and in a footer line at the bottom of Settings, read from the app's package info so it
  always matches the build. Makes "what version are you on?" answerable at a glance.

### Fixed

- **Doubled "v" in version labels.** Catalog sources that already prefix their version with a
  "v" produced `vv1.2.3` on Browse cards (and could on library rows). The label now strips any
  leading "v" before adding its own, so it always reads `v1.2.3`.
- **GitHub "Enable actions" now shows the device code.** Escalating an already-connected,
  read-only account to Star/Watch scope opened GitHub's device page but never surfaced the
  code in Silo — the code display was unreachable while connected. It now renders during the
  read-only → actions escalation.

## [0.2.0] - 2026-08-01

### Added

- **Mod integrity verification (provenance).** The detail drawer can check an installed mod
  against the trusted build SiloAPI hashed from its source: **Verified** (byte-for-byte or
  content match), **Modified** (with the exact list of changed / added / removed files — the
  injected-code candidates), or **Unverified** (no hashed build to compare against yet). The
  verdict is remembered as you move around, and verified/modified mods carry a badge in the
  library list. Byte-exact against the server via a cross-language-ratified manifest format.
  It's a provenance check, not a virus scan — it confirms origin and integrity, and says so.
  Coverage spans GitHub and ModHub sources, popular-first, and grows every hour.
- **Organize preview.** The manual **Organize** button now shows a read-only dry run
  first — exactly which zips move into which `archive/<Category>/` folder — before a
  single file is touched. Confirm to apply, Cancel to touch nothing. Dismissible for
  good with "Don't preview next time" so power users aren't nagged. (The opt-in
  auto-filer still runs silently — you already chose it.)

### Security & reliability (hardening pass)

- **Config & mod-settings edits are confined and atomic.** Editing a game config or a mod's
  settings now writes only inside the FS25 user folder, always succeeds a backup first (it
  won't overwrite if it can't back up), and swaps the new file in atomically — so a
  disk/permission failure can never leave a half-written or unbacked config.
- **Silo only removes files it can prove it created.** Before de-activating or flattening,
  Silo now verifies the file in the mods folder really is its own projection (the hardlink,
  the junction/symlink, or a marked copy). If you swapped in your own build at that name, it
  is left untouched **and now says so** instead of silently doing nothing — closing the last
  realistic data-loss path (a user replacement being removed because the filename matched).
- **Migrated tokens leave no residue.** When a token from an older build is moved from the
  database into the OS keychain, Silo now compacts the database afterward so the old
  plaintext bytes can't be carved back out of it.
- **The backend owns where it writes.** Installs, organizing, and updates now validate the
  target mods folder against the game folder(s) Silo detected, rather than trusting a path
  handed in by the UI — a defence-in-depth boundary on the Rust side.
- **Direct catalog installs are validated like updates — and identity-checked.** A downloaded
  mod must be a fully openable `.zip` containing `modDesc.xml` before it's admitted, so a
  truncated or garbage download can't land as a "broken mod". And when the catalog has a
  canonical hash for the mod (GitHub-source), the downloaded bytes must match it byte-for-byte
  — a valid ZIP of the *wrong* mod, or a tampered asset, is refused.
- **Safer cross-volume moves.** A move that can't be a fast rename now copies to a temporary
  file and swaps it into place, so an interrupted move never leaves a half-written file.
- **Organize can never delete your only copy.** The restore/deactivate paths now
  refuse to remove a file from the mods root unless the archived copy exists behind
  it — so an update or organize interrupted by a crash or power loss leaves your
  original untouched rather than risking the last copy. Added a regression test for
  that exact interrupted state.
- **Mod downloads carry no credentials.** Release assets are public and are now
  fetched unauthenticated; your GitHub token is used only for explicit actions you
  take (star / watch), never attached to a download, and never over plaintext HTTP.
- **Update writes are confined to your mods folder.** The destination is validated to
  resolve inside a real mods root before anything is written.
- **Interrupted updates roll back.** If an in-place update fails mid-write, Silo
  restores the previous version from its backup instead of leaving a truncated file.
- **Hardened XML parsing.** Upgraded `quick-xml` to close two denial-of-service
  advisories (RUSTSEC-2026-0194 / 0195) a malformed mod could otherwise trigger.
- **The CI dependency audit is now a release gate** — the build fails on any known
  vulnerable dependency.

### Changed

- Catalog images now load from SiloAPI's server-side cache when available (faster and
  referer-free), falling back to the in-app proxy for not-yet-cached images.

## [0.1.0] - 2026-07-30

First public beta.

### Added

- **Library scan & categorization** — scans the FS25 mod folder, reads each
  `modDesc.xml`, and categorizes mods (maps, vehicles, scripts, DLC, and more)
  with counts and scan timing. Warm scans are cached and fast (cache keyed by
  path + mtime + size).
- **Zip organizing** — moves loose mod zips into `mods/archive/<Category>/`;
  dev **folder** mods are left untouched (zip-only, never destructive).
- **Symlink/junction projection** — only the active set is projected into the
  game's flat `mods/` folder at launch, via symlink/junction (zips via hardlink)
  with a copy fallback. Organizing moves your zips into a local `archive/` folder,
  fully reversible with one-click Flatten; nothing ever leaves your machine.
- **Loadouts & profiles** — save the current active set as a loadout;
  apply / overwrite / delete; export to a `.silo` file and re-import. Active
  loadout name shows in the toolbar. Build a loadout from a savegame's mod list.
- **Curation** — favorite, hidden, and broken flags; star ratings; tags; and
  free-text notes, all persisted across rescans and usable as filters.
- **Detail drawer** — per-mod icon, metadata, and dependency status
  (present / missing), plus full-body descriptions and changelog, per-mod log
  health, and catalog/update status.
- **Conflict detection** — flags duplicate active maps (an instant-crash cause),
  plus colliding fill types, vehicle/unique types, and scripts across the active
  set, with severity and the mods involved.
- **Crash & log triage** — parses `log.txt`, separates real errors from cosmetic
  noise, and names the mod at fault with a plain-language verdict and ranked
  culprits.
- **Guided bisection** — when the log can't name a culprit, automates the
  "disable half, relaunch" search to isolate it; crash-safe, snapshotting and
  restoring the active set (including recovery after a mid-run force-quit).
- **Bindings map** — parses `inputBinding.xml` into a per-device control map and
  highlights inputs bound to more than one action (a view, not a verdict).
- **Multiplayer mod-set sync** — export a hashed manifest (`.silomp`) of the
  active set; a joiner verifies theirs against it with a fix-list
  (missing / wrong version / different file / extra).
- **Filltype-compatibility bridge generator** — generates a companion mod that
  adds a stubborn map fill type into the categories your equipment accepts (the
  "sugar beet" fix), with no vehicle edits. Output is per-user and reversible.
- **Savegame backup** — copies saves to a backup folder before edits.
- **Cross-source catalog (Browse)** — one canonical record per mod aggregating
  ModHub + GitHub (backed by SiloAPI), with search, category filter,
  sort (popular / downloads / rating / newest / name), and pagination past the
  result cap. In-app GitHub install with a streaming progress bar; ModHub
  are index + open-page.
- **Catalog-routed update checking** — checks the whole library against the
  catalog's real latest-across-sources, fixing the GitHub-vs-ModHub false
  "outdated" reports; per-mod update status also shown in the detail drawer.
- **Per-source interaction** — star/watch a repo on GitHub and
  deep-link to the ModHub page to rate — all through *your own* accounts. Silo
  brokers the action and holds none of your credentials.
- **Cross-platform** — Windows, macOS, and Linux, with per-OS projection and
  game-install discovery. Automatic mods-folder detection covers Windows and macOS;
  on Linux (Proton) the mods folder is set manually for now.

[Unreleased]: https://github.com/HLLMR-VENTURES/silo/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/HLLMR-VENTURES/silo/releases/tag/v0.1.0
