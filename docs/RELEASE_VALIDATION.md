# Release validation records

Per-release evidence that the **packaged** artifact was validated, per `RC_TESTING.md`. Each
entry records the exact artifact, environment, what was tested, and the outcome — closing the
gap between the RC protocol and visible proof it ran.

---

## v1.0.0 — 2026-10-19

### Artifact identity
| Field | Value |
|---|---|
| Version | 1.0.0 (exe FileVersion + ProductVersion + Apps & features `DisplayVersion` agree; Settings › About "Silo 1.0.0") |
| Source | Tag `v1.0.0` on `cc12c1b` (main CI green); release-workflow run 39267015886, all 6 jobs green (macOS arm64 + x64, Linux, Windows signed, SBOM, checksums) |
| Installers | `Silo_1.0.0_x64-setup.exe` SHA-256 `9c2b4bcc7c2a54c544f49499dcea90f4cf7e075392780ea5053c09bcd6ffa3c0`; `Silo_1.0.0_x64_en-US.msi` SHA-256 `0fa3af1b3e13c35b95e5f9d50a6d2b96cd34f3726d2b5acf7c95724a30916334` (both match `SHA256SUMS.txt`) |
| Authenticode | **Valid**, signer `CN=David Hellmer`, timestamped, on both installers |
| Provenance | `gh attestation verify` passes for setup.exe and msi (workflow ref `refs/tags/v1.0.0`) |
| Checksums | `SHA256SUMS.txt` lists all 18 other assets, **including `silo-sbom.cdx.json` and `latest.json`** (v0.11.0's list missed the SBOM; fixed by running `checksums` after `sbom`) |
| Updater manifest | `latest.json` v1.0.0, 11 platform keys each with a signature + a `/download/v1.0.0/` URL; `releases/latest/download/latest.json` serves 1.0.0 after publish |
| Install path | Silent per-user NSIS (`/S`) **over an existing 0.11.0** install at `%LOCALAPPDATA%\silo`; one "Silo" entry in Apps & features afterwards |
| Environment | Windows 11 Pro 26200, standard user, no elevation; real library of **739 mods** (736 organized, 3 active including a dev folder mod), cloud-synced Documents |

### Live tests
- **Upgrade 0.11.0 → 1.0.0 is non-destructive: ✅ PASS.** A size+mtime listing of the whole `mods/` tree (761 entries) before installing vs. after installing, launching, and closing 1.0.0: **0-line diff**. `integrity_check: ok`, `user_version` 2.
- **The release candidate on main, in the real app (dev build), overnight 2026-10-18/19: ✅ PASS after fixes.** A top-down click-through on the real library and on a throwaway demo profile (`SILO_PROFILE_DIR`, first run, organize, mod sets, bulk park + Undo, set switch, Find the problem mod, share `.silo`/`.silomp`, Undo organizing, settings editors). It found 4 blockers and ~30 smaller issues, all fixed before tagging (#268-#300; see Findings). Re-run on main after the fixes; records in the release-check reports.
- **Find the problem mod with unfiled mods: ✅ PASS** (demo profile, 693 loose mods): files them first on consent (Cancel moves nothing), parks the right half on disk (348 of 693 active), the Library shows the round's real set, Stop asks first and restores all 693.
- **Settings editors: ✅ PASS.** A number field (Max shadow lights 10 → 6) saves, with backups; Performance class shows the game's own `Medium`.
- **#266 on real data: ✅ PASS.** "Realistic Soil & Fertilizer" shows in full after the one-time re-parse (0.11 showed "Realistic Soil").

### Findings (all fixed before the tag)
- **Find the problem mod never parked unfiled (loose) mods**, so a newly added mod (the likely culprit) could never be found (#291).
- **Number fields in the settings editors couldn't be saved** (a JS number sent to a Rust `String`) (#278).
- **An integrity verdict, a GitHub update check or a typed note could land on the next mod clicked** (#276, #297).
- **XML text was cut at the first entity** (`Elm &amp; Ash` → "Elm") in modDesc, savegame, store and config reads (#266, #268).
- Plus: a move fallback that could leave two copies of an in-use mod (#279), a pasted GitHub token that could reach the log (#281), unbounded shared sets (#277), the 500 MB install cap (#282), false type/specialization conflicts (#275), the release body's "Beta" and the SBOM missing from the checksums (#269), and many UI and copy fixes.

### Not run live
- The in-app update offer from 0.11.0 (it reads `/releases/latest`, live only after publish).
- The 0.11 → 1.0 database migration on a v1 database: this machine's database was already migrated by the dev builds (with `db-snapshots/silo-before-v1.db` kept); the migration is covered by `db.rs` tests (`migrating_to_v2_keeps_every_set_as_a_user_set`, concurrent first opens).
- macOS and Linux packages: built and signed by CI, not run here (#171).

### Sign-off
- Artifact identity, signing, provenance, upgrade integrity and the release's headline flows: **PASS**. Published as Latest 2026-10-19T14:31:10Z.

---

## v0.8.0 — 2026-10-16

### Artifact identity
| Field | Value |
|---|---|
| Version | 0.8.0 (exe FileVersion + ProductVersion + registry `DisplayVersion` agree) |
| Source | GitHub release `v0.8.0` (non-prerelease "Latest"), release-workflow run 38976382678, all 6 jobs green (macOS arm64 + x64, Linux, Windows signed, SBOM, checksums) |
| Installer | `Silo_0.8.0_x64-setup.exe`, SHA-256 `3aa868c89d25172746b6fb7825d0454516e90cfc9965806ea46c3a3bb053fed4` (matches `SHA256SUMS.txt`) |
| Authenticode | **Valid**, signer `CN=David Hellmer`, Microsoft RSA timestamp; installed `silo.exe` also Valid |
| Updater manifest | `latest.json` v0.8.0, 11 platform keys each with a signature + v0.8.0 URL; Windows key layout identical to v0.7.3 (`windows-x86_64` → MSI, `-nsis` → setup.exe); `releases/latest/download/latest.json` serves 0.8.0 after publish |
| Install path | Silent per-user NSIS (`/S`) **over an existing 0.7.3** install at `%LOCALAPPDATA%\Silo` |
| Environment | Windows 11 Pro 26200, standard user, no elevation; real library of **737 mods** (736 organized, 1 dev folder mod active), cloud-synced Documents (copy projections) |

### Live tests
- **Upgrade 0.7.3 → 0.8.0 is non-destructive: ✅ PASS.** A size+mtime listing of the whole `mods/` tree (741 files) before installing vs. after installing, launching, and closing 0.8.0: **0-line diff**.
- **DB migration (organize/projection scoped per folder): ✅ PASS.** The first 0.8.0 launch rebuilt `organized`/`projection` with `(root, …)` primary keys and claimed all **736** rows for the default folder's key; `integrity_check: ok`; `mod_cache` 737. The library rendered as before (same active set, no banners). `silo.db` was backed up before the first migrating launch.
- **Mods-folder switch (#162) on a debug build of the release code: ✅ PASS** (operator-driven). Change… → empty scratch folder → "Only in Silo" → mismatch banner → "Use the game's folder" restored the library; `gameSettings.xml` MD5 unchanged (`dc8225f8…`).

### Findings
- **#177 (high, fixed in this release):** Cancel was ignored on every confirmation dialog since 2026-07-14. `tauri-plugin-dialog` makes `window.confirm` async, and the bare `if (!confirm(...))` guards got a truthy Promise. Found during the #162 test, when Cancel still rewrote `gameSettings.xml`; restored byte-for-byte from the `.bak`. Fixed in #178 with an awaited `confirmDialog()` and a build guard (`scripts/check-confirm.mjs`).

### Not run live
- The RC_TESTING Phase 2 abuse batteries (kill mid-op, manufactured FS failures) on a sacrificial library copy. Organize/flatten crash-safety is covered by its regression tests; the new per-folder isolation has a unit test (`records_from_one_folder_never_authorize_changes_in_another`).
- macOS and Linux packages: built and signed by CI, not run here (#171; Linux detection is with the #166 reporter).

### Sign-off
- Upgrade integrity, migration and the release's headline flow: **PASS**. Published as Latest.

---

## v0.2.4 — 2026-08-01

### Artifact identity
| Field | Value |
|---|---|
| Version | 0.2.4 (exe version resource + registry `DisplayVersion` agree) |
| Source | GitHub release `v0.2.4` (non-prerelease "Latest"), signed; `latest.json` resolves at the updater endpoint |
| Install path | reached via **in-app auto-update** from 0.2.3 (per-user NSIS at `%LOCALAPPDATA%\Silo`) |
| Environment | Windows 11 Pro 26100, standard user; real library of **728 mods** (organized) |

### Live tests — operator-driven (GUI + kills by owner, forensics scripted)

- **Auto-update 0.2.3 → 0.2.4 — ✅ PASS.** In-app "Update to 0.2.4" downloaded the signed
  release, verified the minisign signature, installed, and relaunched as 0.2.4 (confirmed by
  version resource + registry). Validates the whole updater pipeline end-to-end in the wild.
- **App-update is non-destructive — ✅ PASS.** Forensic diff of the entire `mods/` tree before
  vs. after the update: **byte-identical** (728 files unchanged); flat root unchanged; DB
  `integrity_check: ok`, `mod_cache` = 728. Updating the app touches no mod files.
- **User-ownership / projection guard (RC §D, the crown jewel) — ✅ PASS.** With a mod set
  Active, a *different* file was placed at that mod's managed name in the flat `mods/` root.
  On **launch** (when Silo projects the active set), Silo **did not overwrite or delete** the
  foreign file — it survived byte-for-byte (sha unchanged), and the archived original was
  intact. Whole-tree diff showed no unexplained deletion/change. Library restored to baseline
  after (byte-identical). Confirms the supreme invariant: *Silo never removes a file it can't
  prove it created*, even when that file occupies a projection target.

### Findings
- **#34 (low / enhancement) — silent conflict handling.** Silo preserves a foreign/mismatched
  file at a managed name (data-safe) but doesn't *flag* it in the library view or *report* that
  it skipped the projection. Visibility gap, not a data-safety bug. Tracked.

### Not run live (covered elsewhere)
- **Bad-download interception (RC §C).** Injecting a wrong/corrupt asset mid-download needs a
  network proxy — impractical through the real GUI. Covered by the identity-guard unit tests
  (#30: wrong-mod / non-mod / corrupt all refused) and the catalog installer's hash check.
- **Kill-mid-operation crash recovery (RC §A).** The organizer's interrupted-state safety is
  covered by its regression tests; a live kill-mid-Organize pass is deferred.

### Sign-off
- Hard-stop-class invariants exercised live (auto-update integrity, non-destructive update,
  ownership/projection guard): **PASS**. One low-severity visibility finding (#34).
- Verdict: **suitable for the controlled public beta.** Run the deferred live kill-mid-op pass
  before a broad promotional push if desired.

---

## v0.2.2 — 2026-08-01

### Artifact identity
| Field | Value |
|---|---|
| Version | 0.2.2 (FileVersion + ProductVersion + registry `DisplayVersion` all agree) |
| Source | GitHub release `v0.2.2` (non-prerelease "Latest"), signed; 9 platform assets + `latest.json` |
| Installed binary SHA-256 | `9aaebbdccb7ee7f34379e999fc34cfeeaf5c382ddb3aab40cf45693cc4f7e7ac` (`%LOCALAPPDATA%\Silo\Silo.exe`, 21,170,688 bytes) |
| Install type | Per-user NSIS → `%LOCALAPPDATA%\Silo` |
| Updater manifest | `releases/latest/download/latest.json` reachable (HTTP 200); version 0.2.2; a signature + correct download URL per platform |

> Note: identity confirmed via the Windows version resource + registry, **not** a naïve binary
> string grep — a `grep 0.2.x` on the exe returns a dependency's `0.2.5`, the exact false
> positive `RC_TESTING.md` warns about.

### Environment
- Windows 11 Pro 26100, standard user, no elevation.
- Real library: **728 mods**, organized layout (`mods/archive/<Category>/`), active set projected.

### Static filesystem-integrity audit — ✅ PASS
The "supreme court" check (`RC_TESTING.md`: every byte present, absent, or changed for an
explainable reason), run against the real 728-mod library after the full 0.1.0 → 0.2.2 cycle:

- **728 / 728** archives present; **0** zero-byte; **0** central-directory failures (all 728);
  full-CRC sample (~20 spread across the set) **0** failures.
- **DB `PRAGMA integrity_check: ok`** (snapshot copy).
- **DB ↔ disk consistent:** `mod_cache` = **728** rows == 728 disk archives (no drift).
- Projection in the flat `mods/` root resolves (the user's dev-mod symlink → target OK).
- No orphaned files, no losses, no unexplained deletions.

**Verdict:** no data-loss path exercised in normal steady state; the reversible-projection
model held across a multi-version upgrade on a real library.

### Migration
- DB carries state accumulated across 0.1.0 → 0.2.2; `integrity_check` clean; scan cache
  consistent with disk. Credential migration (token → keychain, DB scrub + VACUUM) was
  validated during the 0.2.0 RC battery; not re-exercised here.

### Observation to investigate (not a hard stop)
- `organized` table has **0** rows while 728 mods are physically organized under `archive/`.
  Every file is present (no loss), but confirm whether Silo sources "Restore vanilla"
  reversibility from disk structure vs. this table, and whether an empty table degrades any
  restore path. Track as a follow-up.

### Pending — interactive Phase 2 battery (requires an operator at the GUI)
Not yet executed against 0.2.2; requires driving the app + force-kills on a **sacrificial**
cloned library (never the real one for destructive ops):
- Kill mid-operation (Organize / Flatten / Activate / Loadout apply / Update / Install / Bisection / Backup / Config save) → forensic diff after each.
- Manufactured filesystem failures (read-only dest/source, locked files, disk full, existing `.bak`/`.part`, source-deleted-between-preview-and-confirm).
- Bad-download abuse (empty / HTML-as-zip / no `modDesc.xml` / truncated / wrong-valid-mod / hash-mismatch).
- User-ownership conflicts (drop a different zip at a projected name; Silo must refuse to delete the user's file).
- Degraded network (SiloAPI down / malformed JSON / timeouts) → library + diagnostics stay usable.
- Fresh-install vs upgrade matrix.

### Sign-off
- Static integrity audit: **PASS** (automated, 2026-08-01).
- Interactive abuse battery: **PENDING** (operator-driven).
- Overall: suitable for **controlled beta**; run the interactive battery before a broad push.
