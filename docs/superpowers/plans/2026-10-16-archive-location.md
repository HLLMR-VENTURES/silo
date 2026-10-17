# Library Archive Location Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let each mods folder keep its archive anywhere (any drive), move an existing archive there safely (verified, resumable, pausable, cancellable, never while FS25 runs), link active mods across drives with symlinks where allowed, and adopt a folder that already holds mods.

**Architecture:** `organize::Layout { mods, archive }` replaces the hard-coded `root.join("archive")` everywhere in `organize` and `scan`. A new `relocate` module owns the move job: a persisted marker in `app_setting`, an idempotent per-mod step (copy → verify → re-point links → delete source), a loop with pause/cancel/game-running checks at every mod boundary, and cancel-by-reversal. `lib.rs` resolves the active layout (including an in-progress move) through the same `modsroot` active-root seam #176 introduced, and blocks all write commands while a move is unfinished.

**Tech Stack:** Rust (Tauri v2, rusqlite, serde_json, new dep `sysinfo` 0.36 with `system` + `disk` features), Svelte 5 + TypeScript.

**Spec:** `docs/superpowers/specs/2026-10-16-archive-location-design.md` (#166).

---

## Conventions for every task

- Branch `feat/archive-location`; one commit per task (Tasks 2-3 and 9-10 commit together where noted). Commit messages end with the `Co-Authored-By` trailer the session specifies.
- **Run cargo from PowerShell**, not Git Bash (Git's `link.exe` shadows MSVC there).
- **Write code with the editor, never shell heredocs** — heredocs collapse `\\` in Rust raw strings.
- Gate before the PR: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` (in `src-tauri/`), `npm run build` (repo root; it also runs `scripts/check-confirm.mjs` — every confirmation must use `await confirmDialog(...)`).
- `sysinfo` resolves to **0.36.x** (our `rust-version = "1.77"` keeps the MSRV-aware resolver below 0.39, which needs 1.95). The APIs used below were checked against 0.36.1's source.
- Never test destructive steps on the real library. Use #176's folder switcher on a disposable mods folder.

## File map

| File | Change | Responsibility |
|---|---|---|
| `src-tauri/Cargo.toml` | modify | add `sysinfo = { version = "0.36", default-features = false, features = ["system", "disk"] }` |
| `src-tauri/src/gamelaunch.rs` | modify | `is_fs25_process`, `fs25_running` |
| `src-tauri/src/organize.rs` | modify | `Layout`; every engine fn takes `&Layout`; `apply_organize`/`plan_organize` take a source folder; zip symlink projection; `supports_symlinks`; relink upgrades copies to symlinks across drives; a few helpers become `pub(crate)` |
| `src-tauri/src/scan.rs` | modify | `scan_layouts` (explicit archive list, de-duplicated); `scan_cached` becomes a wrapper |
| `src-tauri/src/modsroot.rs` | modify | the published active root becomes an `Active { layout, move_to, move_state }` |
| `src-tauri/src/relocate.rs` | **create** | marker, preflight, per-mod step, run loop, cancel, adopt preview |
| `src-tauri/src/lib.rs` | modify | `primary_layout`, write guard, archive setting resolution, scan archives, organize call sites, archive commands, launch guard |
| `src/lib/api.ts` | modify | archive types + command wrappers + progress listener |
| `src/lib/components/panels/SettingsPanel.svelte` | modify | Library archive section |
| `src/lib/components/panels/Topbar.svelte` | modify | `launchBlocked` prop |
| `src/App.svelte` | modify | archive status, banners, move/pause/resume/cancel, adopt preview, launch block |
| docs | modify | CHANGELOG, SMOKE_TEST, CROSS-PLATFORM, CLAUDE.md, spec notes |

---

### Task 1: Detect a running Farming Simulator 25

**Files:**
- Modify: `src-tauri/Cargo.toml` (dependencies)
- Modify: `src-tauri/src/gamelaunch.rs`

- [ ] **Step 1: Add the dependency.** In `src-tauri/Cargo.toml` `[dependencies]`, add:

```toml
sysinfo = { version = "0.36", default-features = false, features = ["system", "disk"] }
```

Run (PowerShell, `src-tauri/`): `cargo update -p sysinfo` then `cargo tree -i sysinfo --depth 0`. Expected: `sysinfo v0.36.x`.

- [ ] **Step 2: Write the failing tests** — append to `gamelaunch.rs`'s test module (create `#[cfg(test)] mod process_tests { use super::*; … }` at the end of the file if the existing module name differs):

```rust
#[cfg(test)]
mod process_tests {
    use super::*;

    #[test]
    fn matches_fs25_processes_on_every_platform() {
        // Windows
        assert!(is_fs25_process("FarmingSimulator2025.exe", &[]));
        assert!(is_fs25_process("FarmingSimulator2025Game.exe", &[]));
        assert!(is_fs25_process("farmingsimulator2025game.exe", &[]));
        // Proton: Linux truncates comm to 15 chars; the command line carries the exe path.
        assert!(is_fs25_process(
            "FarmingSimulato",
            &["Z:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25\\x64\\FarmingSimulator2025Game.exe".into()]
        ));
        // macOS
        assert!(is_fs25_process("FarmingSimulator2025Game", &[]));
        assert!(is_fs25_process("Farming Simulator 25", &[]));
        // Dedicated server only when it lives in an FS25 install.
        assert!(is_fs25_process(
            "dedicatedServer.exe",
            &["E:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25\\dedicatedServer.exe".into()]
        ));
        assert!(!is_fs25_process("dedicatedServer.exe", &["C:\\OtherGame\\dedicatedServer.exe".into()]));
    }

    #[test]
    fn ignores_unrelated_processes() {
        assert!(!is_fs25_process("silo.exe", &[]));
        assert!(!is_fs25_process("FarmingSimulator2022.exe", &[]));
        assert!(!is_fs25_process("explorer.exe", &["C:\\Windows\\explorer.exe".into()]));
    }
}
```

- [ ] **Step 3: Run to verify failure.** `cargo test process_tests` → compile error, `is_fs25_process` not found.

- [ ] **Step 4: Implement** — add near the top of `gamelaunch.rs` (after the existing `use` lines):

```rust
/// True for an FS25 process (launcher, game, or the dedicated server shipped in an FS25 install).
/// `name` is the process name (truncated to 15 chars on Linux); `cmd` is its command line.
/// Pure so it unit-tests without a running game.
pub fn is_fs25_process(name: &str, cmd: &[String]) -> bool {
    let n = name.to_ascii_lowercase();
    let line = cmd.join(" ").to_ascii_lowercase();
    let game = |s: &str| {
        s.contains("farmingsimulator2025") || s.contains("farming simulator 25")
    };
    if n.starts_with("dedicatedserver") {
        return line.contains("farming simulator 25") || line.contains("farmingsimulator2025");
    }
    game(&n) || (n.starts_with("farmingsimulato") && game(&line))
}

/// True if FS25 (or its dedicated server) is running right now. Used to refuse moving mods the
/// game may have open.
pub fn fs25_running() -> bool {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    sys.processes().values().any(|p| {
        let cmd: Vec<String> = p.cmd().iter().map(|s| s.to_string_lossy().into_owned()).collect();
        is_fs25_process(&p.name().to_string_lossy(), &cmd)
    })
}
```

- [ ] **Step 5: Run tests.** `cargo test process_tests` → `2 passed`. Then `cargo clippy --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/gamelaunch.rs
git commit -m "feat(gamelaunch): detect a running FS25 (launcher, game, dedicated server)"
```

---

### Task 2: `organize::Layout` — archive location as a parameter

**Files:**
- Modify: `src-tauri/src/organize.rs` (whole engine; tests)
- Modify: `src-tauri/src/lib.rs` (organize call sites, temporarily via `Layout::default_for`)
- Modify: `src-tauri/examples/organize_test.rs`, `src-tauri/examples/organize_real.rs` (call sites)

This task is a behavior-preserving refactor: every existing test must still pass with `Layout::default_for(root)`.

- [ ] **Step 1: Add the type** below the `ARCHIVE` const in `organize.rs`:

```rust
/// Where a mods folder's pieces live. `mods` is the flat folder the game reads; `archive` holds
/// parked mods as `<archive>/<Category>/<file>`. Default: `<mods>/archive` (#166 makes it
/// configurable per mods folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub mods: PathBuf,
    pub archive: PathBuf,
}

impl Layout {
    pub fn default_for(mods: &Path) -> Layout {
        Layout {
            mods: mods.to_path_buf(),
            archive: mods.join(ARCHIVE),
        }
    }

    /// True when the archive is the default `<mods>/archive` (Silo may remove it when empty).
    pub fn archive_is_default(&self) -> bool {
        self.archive == self.mods.join(ARCHIVE)
    }
}
```

- [ ] **Step 2: Re-base the path helpers on the archive folder:**

```rust
pub(crate) fn archive_dir(archive: &Path, category: &str) -> PathBuf {
    archive.join(sanitize(category))
}

pub(crate) fn archive_path(archive: &Path, category: &str, file_name: &str) -> PathBuf {
    archive_dir(archive, category).join(file_name)
}
```

- [ ] **Step 3: Change every engine function's `root: &Path` parameter to `layout: &Layout`.** In each, add `let root = &layout.mods;` as the first line (so existing `root.join(&file_name)` flat-root code is untouched), and change every `archive_path(root, …)` / `archive_dir(root, …)` to `archive_path(&layout.archive, …)` / `archive_dir(&layout.archive, …)`. The functions: `apply_organize`, `set_active`, `relink_projections`, `adopt_foreign`, `restore_projection`, `flatten`, `detect_foreign_projections`. The `let key = crate::modsroot::canonical_key(root);` line stays (it now reads `layout.mods` through `root`). `backup_path(root, …)` is unchanged (backups stay in the mods folder).

- [ ] **Step 4: `flatten` cleanup and `detect_foreign_projections` archive reads.**
  - In `flatten`, replace `cleanup_empty_archive(&root.join(ARCHIVE))` with `cleanup_empty_archive(&layout.archive, layout.archive_is_default())`.
  - Change `cleanup_empty_archive` to never remove a user-chosen archive folder itself:

```rust
fn cleanup_empty_archive(archive: &Path, remove_root: bool) -> std::io::Result<Vec<PathBuf>> {
```

  and at its end replace `if walk(archive, &mut leftover)? { let _ = std::fs::remove_dir(archive); }` with:

```rust
    if walk(archive, &mut leftover)? && remove_root {
        let _ = std::fs::remove_dir(archive);
    }
```

  Make it `pub(crate)` (the move job reuses it).
  - In `detect_foreign_projections`, replace `let archive = root.join(ARCHIVE);` with `let archive = layout.archive.clone();`.

- [ ] **Step 5: `plan_organize` — keep its signature for now** (Task 3 changes it). Its `rel_to` string stays `archive/<Category>/<file>`.

- [ ] **Step 6: Update the organize tests.** Each test passes a temp folder as root (`&base`, `&root`, `&a`, `&b`). Replace engine calls of the form `fn_name(&conn, &X, …)` with `fn_name(&conn, &Layout::default_for(&X), …)` for `set_active`, `apply_organize`, `flatten`, `relink_projections`, `adopt_foreign`, `restore_projection`, `detect_foreign_projections`. `cleanup_empty_archive(&p)` calls in tests become `cleanup_empty_archive(&p, true)`.

- [ ] **Step 7: Add a test that a custom archive is honored** (end of `organize.rs` tests):

```rust
    #[test]
    fn organize_activate_and_flatten_honor_a_custom_archive() {
        let base = std::env::temp_dir().join(format!("silo_customarch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("elsewhere");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&arch).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout { mods: mods.clone(), archive: arch.clone() };
        std::fs::write(mods.join("FS25_Foo.zip"), b"PK\x03\x04 foo").unwrap();
        let input = ModInput {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
        };

        let rep = apply_organize(&conn, &layout, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        assert!(arch.join("Vehicles/FS25_Foo.zip").exists(), "filed into the custom archive");
        assert!(!mods.join("archive").exists(), "default archive untouched");

        let on: HashSet<String> = ["FS25_Foo".to_string()].into_iter().collect();
        set_active(&conn, &layout, &on);
        assert!(mods.join("FS25_Foo.zip").exists(), "projected into the mods folder");

        flatten(&conn, &layout);
        assert!(mods.join("FS25_Foo.zip").exists(), "restored to the mods folder");
        assert!(arch.exists(), "a user-chosen archive folder is never removed");
        let _ = std::fs::remove_dir_all(&base);
    }
```

(`apply_organize` gets its `source` parameter in Task 3; until then this call has no source argument.)

- [ ] **Step 8: Update `lib.rs` call sites mechanically** (real layout resolution comes in Task 8). In each command that calls an engine function, wrap the trusted root: `organize::set_active(&conn, &organize::Layout::default_for(&root), &set)`, likewise for `apply_organize`, `adopt_foreign`, `restore_projection`, `relink_projections`, `flatten`, `detect_foreign_projections` (lines ~908-1004 and ~2283). Examples: in `examples/organize_test.rs` and `examples/organize_real.rs`, wrap their root the same way.

- [ ] **Step 9: Run the suite.** `cargo test` → all pass (127 existing + 1 new). `cargo clippy --all-targets -- -D warnings` → clean. Do not commit yet (Task 3 finishes the refactor).

---

### Task 3: Organize from a chosen source folder (for adopting)

**Files:**
- Modify: `src-tauri/src/organize.rs` (`plan_organize`, `apply_organize`, tests)
- Modify: `src-tauri/src/lib.rs` (two call sites)

- [ ] **Step 1: Write the failing test** (organize tests):

```rust
    #[test]
    fn adopting_files_loose_archive_mods_into_categories() {
        let base = std::env::temp_dir().join(format!("silo_adopt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("hdd");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&arch).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout { mods: mods.clone(), archive: arch.clone() };
        std::fs::write(arch.join("FS25_Loose.zip"), b"PK\x03\x04 loose").unwrap();
        let input = ModInput {
            tech_name: "FS25_Loose".into(),
            file_name: "FS25_Loose.zip".into(),
            kind: "zip".into(),
            category: "Tools".into(),
            subcategory: None,
        };

        let plan = plan_organize(&arch, std::slice::from_ref(&input));
        assert_eq!(plan.len(), 1);
        let rep = apply_organize(&conn, &layout, &arch, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        assert!(arch.join("Tools/FS25_Loose.zip").exists());
        assert!(!arch.join("FS25_Loose.zip").exists());
        assert!(!mods.join("FS25_Loose.zip").exists(), "adopted mods arrive parked");
        let _ = std::fs::remove_dir_all(&base);
    }
```

- [ ] **Step 2: Run to verify failure.** `cargo test adopting_files` → arity error on `apply_organize`.

- [ ] **Step 3: Implement.** `plan_organize(source: &Path, mods: &[ModInput])` — rename its `root` parameter to `source` (body: `let from = source.join(&m.file_name);`). `apply_organize(conn, layout, source: &Path, mods)` — `let from = source.join(&m.file_name);` (was `root.join`). Update the doc comments: "move mods from `source` (the flat mods folder, or the archive's top level when adopting) into `<archive>/<Category>/`".

- [ ] **Step 4: Update callers.** Existing tests/examples/lib: pass the mods folder as `source` (e.g. `apply_organize(&conn, &layout, &layout.mods, &mods)`; in `lib.rs` `plan_organize(&root, &mods)` is unchanged because `root` is the mods folder). Update Task 2's custom-archive test call to `apply_organize(&conn, &layout, &mods, …)`.

- [ ] **Step 5: Run.** `cargo test` → all pass; clippy clean.

- [ ] **Step 6: Commit Tasks 2-3.**

```bash
git add src-tauri/src/organize.rs src-tauri/src/lib.rs src-tauri/examples/organize_test.rs src-tauri/examples/organize_real.rs
git commit -m "refactor(organize): Layout { mods, archive } and a source folder for organize"
```

---

### Task 4: Symlink projection for zips across drives

**Files:**
- Modify: `src-tauri/src/organize.rs` (`make_link`, `supports_symlinks`, `set_active`, `adopt_foreign`, `restore_projection`, `relink_projections`, `RelinkReport`, tests)

- [ ] **Step 1: Write the failing tests:**

```rust
    #[test]
    fn zip_projection_falls_back_hardlink_then_symlink_then_copy() {
        let base = std::env::temp_dir().join(format!("silo_zlink_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("a.zip");
        std::fs::write(&src, b"PK\x03\x04 a").unwrap();

        // Hardlink refused, symlink allowed → a symlink resolving to src.
        let l1 = base.join("l1.zip");
        link_zip(&src, &l1, true, |_, _| Err(std::io::ErrorKind::Other.into()), |s, l| symlink_file(s, l))
            .unwrap();
        assert!(l1.symlink_metadata().unwrap().file_type().is_symlink() || !cfg!(unix));
        assert!(is_silo_projection(&l1, &src, "zip"));

        // Hardlink and symlink refused → a byte copy.
        let l2 = base.join("l2.zip");
        link_zip(
            &src,
            &l2,
            true,
            |_, _| Err(std::io::ErrorKind::Other.into()),
            |_, _| Err(std::io::ErrorKind::Other.into()),
        )
        .unwrap();
        assert_eq!(std::fs::read(&l2).unwrap(), b"PK\x03\x04 a");

        // Symlinks not allowed → never tried.
        let l3 = base.join("l3.zip");
        link_zip(
            &src,
            &l3,
            false,
            |_, _| Err(std::io::ErrorKind::Other.into()),
            |_, _| panic!("symlink must not be attempted"),
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_foreign_file_is_never_mistaken_for_a_symlink_projection() {
        let base = std::env::temp_dir().join(format!("silo_zforeign_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("a.zip");
        std::fs::write(&src, b"PK\x03\x04 a").unwrap();
        let theirs = base.join("theirs.zip");
        std::fs::write(&theirs, b"PK\x03\x04 different").unwrap();
        assert!(!is_silo_projection(&theirs, &src, "zip"));
        let _ = std::fs::remove_dir_all(&base);
    }
```

(On Windows without Developer Mode `symlink_file` fails; the first assertion is written to pass either way, and the copy/never-tried cases are platform-independent.)

- [ ] **Step 2: Run to verify failure.** `cargo test zip_projection` → `link_zip` / `symlink_file` not found.

- [ ] **Step 3: Implement** in `organize.rs`:

```rust
/// Create a file symlink at `link` pointing to `src` (Windows needs Developer Mode or admin).
pub(crate) fn symlink_file(src: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(src, link)
    }
}

/// Zip projection: hardlink (same drive, no duplication) → symlink (other drive, when allowed)
/// → byte copy. The link strategies are injected so the order is unit-testable.
fn link_zip(
    src: &Path,
    link: &Path,
    allow_symlink: bool,
    hard: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    soft: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if hard(src, link).is_ok() {
        return Ok(());
    }
    if allow_symlink && soft(src, link).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, link).map(|_| ())
}

/// Whether a symlink from the mods folder into the archive works here (always on Linux/macOS
/// for normal drives; on Windows only with Developer Mode). Cheap one-shot probe.
pub fn supports_symlinks(layout: &Layout) -> bool {
    let target = layout.archive.join(".silo-symlink-probe");
    let link = layout.mods.join(".silo-symlink-probe.link");
    let _ = std::fs::remove_file(&link);
    if std::fs::create_dir_all(&layout.archive).is_err() || std::fs::write(&target, b"silo").is_err() {
        return false;
    }
    let ok = symlink_file(&target, &link).is_ok();
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&target);
    ok
}
```

Change `make_link(src, link, kind)` to `pub(crate) fn make_link(src: &Path, link: &Path, kind: &str, allow_symlink: bool)`; its zip branch becomes `link_zip(src, link, allow_symlink, |s, l| std::fs::hard_link(s, l), |s, l| symlink_file(s, l))`. Make `remove_link` `pub(crate)` too.

`set_active`, `adopt_foreign`, `restore_projection` compute `let allow_symlink = supports_symlinks(layout);` once at the top and pass it to `make_link`. (`supports_symlinks` writes a probe file; calling it once per operation is cheap.)

`is_silo_projection` already handles symlinks (canonicalized target == src) — no change. `file_stats` returns `None` for a symlink, so no size+mtime record is written for symlinked zips; deactivation identifies them structurally (cheap), as for dir links.

- [ ] **Step 4: Optimize links across drives.** In `relink_projections`, replace the early `if !supports_hardlinks(root) { rep.hardlinks_unsupported = true; return rep; }` with:

```rust
    let hardlinks = supports_hardlinks(root) && same_volume(&layout.mods, &layout.archive);
    let symlinks = !hardlinks && supports_symlinks(layout);
    if !hardlinks && !symlinks {
        rep.hardlinks_unsupported = true;
        return rep;
    }
```

and in the per-row upgrade replace `relink_over(&src, &flat)` with `relink_over(&src, &flat, hardlinks)`, changing `relink_over` to:

```rust
fn relink_over(src: &Path, flat: &Path, hard: bool) -> std::io::Result<()> {
    let tmp = flat.with_file_name(format!(
        "{}.silo-relink-tmp",
        flat.file_name().and_then(|n| n.to_str()).unwrap_or("mod")
    ));
    let _ = std::fs::remove_file(&tmp);
    if hard {
        std::fs::hard_link(src, &tmp)?;
    } else {
        symlink_file(src, &tmp)?;
    }
    match std::fs::rename(&tmp, flat) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}
```

Add the helper (same-volume check via `sysinfo` disks, longest mount point):

```rust
/// Mount point holding `p` (longest matching prefix), for same-drive checks and free space.
pub(crate) fn mount_of(p: &Path) -> Option<(PathBuf, u64)> {
    let c = std::fs::canonicalize(p).ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| c.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().components().count())
        .map(|d| (d.mount_point().to_path_buf(), d.available_space()))
}

pub(crate) fn same_volume(a: &Path, b: &Path) -> bool {
    match (mount_of(a), mount_of(b)) {
        (Some((ma, _)), Some((mb, _))) => ma == mb,
        _ => true, // unknown → assume same (hardlink attempt then falls back safely)
    }
}
```

Update the existing relink tests if they call `relink_over` directly (pass `true`).

- [ ] **Step 5: Run.** `cargo test` → all pass; clippy clean.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src/organize.rs
git commit -m "feat(organize): symlink projection for zips across drives; Optimize links upgrades to symlinks"
```

---

### Task 5: Scan explicit archive folders

**Files:**
- Modify: `src-tauri/src/scan.rs` (`collect_archive_candidates`, `scan_cached`, new `scan_layouts`, tests)

- [ ] **Step 1: Write the failing test** (add a `#[cfg(test)] mod layout_tests` at the end of `scan.rs`):

```rust
#[cfg(test)]
mod layout_tests {
    use super::*;

    fn moddesc_zip(path: &std::path::Path, title: &str) {
        let f = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        w.start_file("modDesc.xml", zip::write::SimpleFileOptions::default()).unwrap();
        use std::io::Write;
        write!(w, "<modDesc descVersion=\"92\"><title><en>{title}</en></title><version>1.0.0.0</version></modDesc>").unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn reads_two_archives_without_duplicating_a_mod() {
        let base = std::env::temp_dir().join(format!("silo_scan2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let new_arch = base.join("new");
        let old_arch = base.join("old");
        for d in [&mods, &new_arch.join("Tools"), &old_arch.join("Tools")] {
            std::fs::create_dir_all(d).unwrap();
        }
        moddesc_zip(&new_arch.join("Tools/FS25_Moved.zip"), "Moved");
        moddesc_zip(&old_arch.join("Tools/FS25_Moved.zip"), "Moved");
        moddesc_zip(&old_arch.join("Tools/FS25_NotYet.zip"), "Not yet");

        let out = scan_layouts(
            vec![(mods.clone(), vec![new_arch.clone(), old_arch.clone()])],
            &HashMap::new(),
            |_, _| {},
        );
        let names: Vec<_> = out.result.mods.iter().map(|m| m.tech_name.clone()).collect();
        assert_eq!(names.iter().filter(|n| *n == "FS25_Moved").count(), 1);
        assert!(names.contains(&"FS25_NotYet".to_string()));
        let moved = out.result.mods.iter().find(|m| m.tech_name == "FS25_Moved").unwrap();
        assert!(moved.path.contains("new"), "the new archive wins");
        let _ = std::fs::remove_dir_all(&base);
    }
}
```

- [ ] **Step 2: Run to verify failure.** `cargo test layout_tests` → `scan_layouts` not found.

- [ ] **Step 3: Implement.** Change `collect_archive_candidates(root: &Path)` to take the archive folder directly: `fn collect_archive_candidates(archive: &Path) -> Vec<Candidate>` and delete its `let archive = root.join("archive");` line (use the parameter). Then add `scan_layouts` and make `scan_cached` a wrapper:

```rust
/// Core scan over mods folders with explicit archive folders. Archives are read in order and a
/// techName already seen in an earlier archive is skipped (during a paused archive move the new
/// archive is listed first). No Tauri dependency — testable / CLI-reusable.
pub fn scan_layouts<F>(
    layouts: Vec<(PathBuf, Vec<PathBuf>)>,
    cache: &HashMap<String, CacheEntry>,
    progress: F,
) -> ScanOutput
where
    F: Fn(usize, usize) + Sync + Send,
{
    let started = std::time::Instant::now();
    let mut root_cands: Vec<Candidate> = Vec::new();
    let mut archive_cands: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (root, archives) in &layouts {
        root_cands.extend(collect_root_candidates(root));
        for a in archives {
            for c in collect_archive_candidates(a) {
                if seen.insert(c.tech_name.clone()) {
                    archive_cands.push(c);
                }
            }
        }
    }
    let roots: Vec<PathBuf> = layouts.into_iter().map(|(r, _)| r).collect();
    finish_scan(roots, root_cands, archive_cands, cache, progress, started)
}

/// Scan with each root's default archive (`<root>/archive`).
pub fn scan_cached<F>(roots: Vec<PathBuf>, cache: &HashMap<String, CacheEntry>, progress: F) -> ScanOutput
where
    F: Fn(usize, usize) + Sync + Send,
{
    let layouts = roots
        .into_iter()
        .map(|r| {
            let a = r.join("archive");
            (r, vec![a])
        })
        .collect();
    scan_layouts(layouts, cache, progress)
}
```

Move the body of the old `scan_cached` after its candidate collection (from `// Tech-names present in the archive…` through building `ScanOutput`) into:

```rust
fn finish_scan<F>(
    roots: Vec<PathBuf>,
    root_cands: Vec<Candidate>,
    archive_cands: Vec<Candidate>,
    cache: &HashMap<String, CacheEntry>,
    progress: F,
    started: std::time::Instant,
) -> ScanOutput
where
    F: Fn(usize, usize) + Sync + Send,
{
    // (old body, unchanged)
}
```

`collect_root_candidates` keeps skipping a top-level `archive` and `backups` folder.

- [ ] **Step 4: Run.** `cargo test` → all pass (including the existing scan tests and `examples` compiling via `cargo clippy --all-targets`).

- [ ] **Step 5: Commit.**

```bash
git add src-tauri/src/scan.rs
git commit -m "feat(scan): scan explicit archive folders (deduped) via scan_layouts"
```

---

### Task 6: Publish the active layout (and any move) from `modsroot`

**Files:**
- Modify: `src-tauri/src/modsroot.rs` (`ACTIVE`, `set_active`, `active`, test)

- [ ] **Step 1: Update the test** `active_root_round_trips`:

```rust
    #[test]
    fn active_layout_round_trips() {
        let l = crate::organize::Layout::default_for(Path::new("/x"));
        set_active(Some(Active { layout: l.clone(), move_to: None, move_running: false }));
        assert_eq!(active().map(|a| a.layout), Some(l));
        set_active(None);
        assert!(active().is_none());
    }
```

- [ ] **Step 2: Implement** — replace the `ACTIVE` static and its two functions:

```rust
/// The trusted write targets, published by the mods-root/archive commands: the active layout
/// and, while an archive move is unfinished, its other archive. `lib::allowed_roots` reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Active {
    pub layout: crate::organize::Layout,
    /// The other archive of an unfinished move (its destination; or its origin when cancelling).
    pub move_to: Option<PathBuf>,
    /// A move is copying right now (not paused) — Launch is blocked.
    pub move_running: bool,
}

static ACTIVE: RwLock<Option<Active>> = RwLock::new(None);

pub fn set_active(a: Option<Active>) {
    if let Ok(mut g) = ACTIVE.write() {
        *g = a;
    }
}

pub fn active() -> Option<Active> {
    ACTIVE.read().ok().and_then(|g| g.clone())
}
```

Fix the one `lib.rs` caller that won't compile yet by leaving it to Task 8 (expected compile errors in `lib.rs` until then — run `cargo test modsroot` with `--lib` only after Task 8, or do Tasks 6-8 back to back).

- [ ] **Step 3:** Continue directly to Task 7; commit Tasks 6-8 together after Task 8.

---

### Task 7: The `relocate` module (move job)

**Files:**
- Create: `src-tauri/src/relocate.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod relocate;` to the module list, alphabetical)

- [ ] **Step 1: Write the module with its tests first** — create `src-tauri/src/relocate.rs`:

```rust
//! Moving a mods folder's archive to a new location (#166). Each mod is copied, verified, its
//! links re-pointed, then its old copy deleted — so every mod boundary is a safe point (the mods
//! folder is complete and valid). Progress survives restarts through a marker in `app_setting`;
//! a move can pause (FS25 may be launched while paused), resume, or cancel (moves back).

use crate::db::{self, OrganizedRow};
use crate::organize::{self, Layout};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

const TMP_SUFFIX: &str = ".silo-move-tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveState {
    Running,
    Paused,
    Cancelling,
}

/// Persisted while a move is unfinished. `from` is where the archive lives now; `to` is where
/// it's going. Cancelling swaps the direction but keeps `from`/`to` as originally chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveMarker {
    pub from: PathBuf,
    pub to: PathBuf,
    pub state: MoveState,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub pause_reason: Option<String>,
}

pub fn archive_setting_key(mods_key: &str) -> String {
    format!("archive_root:{mods_key}")
}

fn marker_key(mods_key: &str) -> String {
    format!("archive_move:{mods_key}")
}

pub fn load_marker(conn: &Connection, mods_key: &str) -> Option<MoveMarker> {
    db::get_app_setting(conn, &marker_key(mods_key)).and_then(|j| serde_json::from_str(&j).ok())
}

pub fn save_marker(conn: &Connection, mods_key: &str, m: &MoveMarker) -> Result<(), String> {
    let j = serde_json::to_string(m).map_err(|e| e.to_string())?;
    db::set_app_setting(conn, &marker_key(mods_key), Some(&j))
}

fn clear_marker(conn: &Connection, mods_key: &str) -> Result<(), String> {
    db::set_app_setting(conn, &marker_key(mods_key), None)
}

// One move at a time. 0 = none, 1 = pause, 2 = cancel.
static REQUEST: AtomicU8 = AtomicU8::new(0);

pub fn request_pause() {
    REQUEST.store(1, Ordering::SeqCst);
}
pub fn request_cancel() {
    REQUEST.store(2, Ordering::SeqCst);
}
fn take_request() -> u8 {
    REQUEST.swap(0, Ordering::SeqCst)
}
fn requested() -> bool {
    REQUEST.load(Ordering::SeqCst) != 0
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
    pub cancelling: bool,
}

/// Size of a mod on disk (file, or a folder recursively).
fn size_of(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p)
            .map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Copy a file in chunks, giving up (and deleting the partial) when `abort()` turns true.
fn copy_file_abortable(src: &Path, dst: &Path, abort: &dyn Fn() -> bool) -> std::io::Result<()> {
    let mut r = std::fs::File::open(src)?;
    let mut w = std::fs::File::create(dst)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if abort() {
            drop(w);
            let _ = std::fs::remove_file(dst);
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "move paused"));
        }
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
    }
    w.sync_all()
}

fn copy_dir_abortable(src: &Path, dst: &Path, abort: &dyn Fn() -> bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_abortable(&e.path(), &to, abort)?;
        } else {
            copy_file_abortable(&e.path(), &to, abort)?;
        }
    }
    Ok(())
}

/// Same content? Zips: SHA-256. Folders: same file count and total size, recursively.
fn verify(src: &Path, dst: &Path, kind: &str) -> bool {
    if kind == "dir" {
        fn tally(p: &Path) -> (usize, u64) {
            std::fs::read_dir(p)
                .map(|rd| {
                    rd.flatten().fold((0, 0), |(n, s), e| {
                        if e.path().is_dir() {
                            let (a, b) = tally(&e.path());
                            (n + a, s + b)
                        } else {
                            (n + 1, s + e.metadata().map(|m| m.len()).unwrap_or(0))
                        }
                    })
                })
                .unwrap_or((0, 0))
        }
        dst.is_dir() && tally(src) == tally(dst)
    } else {
        matches!(
            (crate::provenance::sha256_file(src), crate::provenance::sha256_file(dst)),
            (Ok(a), Ok(b)) if a == b
        )
    }
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(_) => Ok(()),
    }
}

/// If the mod is active and its mods-folder entry is a symlink into `src`, point it at `dst`.
/// Hardlinked/copied zips and marker-copied folders are standalone and need nothing.
fn repoint(row: &OrganizedRow, mods: &Path, src: &Path, dst: &Path, allow_symlink: bool) -> Result<(), String> {
    if !row.active {
        return Ok(());
    }
    let link = mods.join(&row.file_name);
    let Ok(m) = link.symlink_metadata() else {
        return Ok(());
    };
    if !m.file_type().is_symlink() {
        return Ok(());
    }
    let target = std::fs::read_link(&link).map_err(|e| e.to_string())?;
    let points_at_src = target == src || std::fs::canonicalize(&link).ok() == std::fs::canonicalize(src).ok();
    if !points_at_src && link.exists() {
        return Ok(()); // a link to something else — not ours to change
    }
    organize::remove_link(&link).map_err(|e| format!("unlink: {e}"))?;
    organize::make_link(dst, &link, &row.kind, allow_symlink).map_err(|e| format!("relink: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Moved,
    AlreadyDone,
}

/// Move one archived mod from `from` to `to` (archive folders). Idempotent: safe to re-run after
/// a crash, pause, or failure. Never deletes the source until the destination is verified and
/// the mod's link (if any) points at it.
pub fn move_one(
    row: &OrganizedRow,
    mods: &Path,
    from: &Path,
    to: &Path,
    allow_symlink: bool,
    abort: &dyn Fn() -> bool,
) -> Result<Step, String> {
    let src = organize::archive_path(from, &row.category, &row.file_name);
    let dst = organize::archive_path(to, &row.category, &row.file_name);
    let tmp = dst.with_file_name(format!("{}{TMP_SUFFIX}", row.file_name));
    let (s, d) = (src.symlink_metadata().is_ok(), dst.symlink_metadata().is_ok());
    if !s && d {
        repoint(row, mods, &src, &dst, allow_symlink)?;
        return Ok(Step::AlreadyDone);
    }
    if !s && !d {
        return Err("the archived copy is missing from both locations".into());
    }
    if d && !verify(&src, &dst, &row.kind) {
        remove_any(&dst).map_err(|e| format!("clear bad copy: {e}"))?;
    }
    if dst.symlink_metadata().is_err() {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let _ = remove_any(&tmp);
        let copied = if row.kind == "dir" {
            copy_dir_abortable(&src, &tmp, abort)
        } else {
            copy_file_abortable(&src, &tmp, abort)
        };
        if let Err(e) = copied {
            let _ = remove_any(&tmp);
            return Err(e.to_string());
        }
        std::fs::rename(&tmp, &dst).map_err(|e| {
            let _ = remove_any(&tmp);
            format!("place: {e}")
        })?;
        if !verify(&src, &dst, &row.kind) {
            let _ = remove_any(&dst);
            return Err("the copy didn't match the original".into());
        }
    }
    repoint(row, mods, &src, &dst, allow_symlink)?;
    remove_any(&src).map_err(|e| format!("remove old copy: {e}"))?;
    Ok(Step::Moved)
}

/// Delete stale temp copies left by a crash, under each category folder of `archive`.
fn clean_temps(archive: &Path) {
    if let Ok(cats) = std::fs::read_dir(archive) {
        for c in cats.flatten() {
            if let Ok(es) = std::fs::read_dir(c.path()) {
                for e in es.flatten() {
                    if e.file_name().to_string_lossy().ends_with(TMP_SUFFIX) {
                        let _ = remove_any(&e.path());
                    }
                }
            }
        }
    }
}

pub struct RunCtx<'a> {
    pub conn: &'a Connection,
    pub mods: &'a Path,
    pub mods_key: &'a str,
    pub game_running: &'a dyn Fn() -> bool,
    pub progress: &'a mut dyn FnMut(&Progress),
}

/// Run (or resume / cancel) the move described by `marker` until it finishes or pauses.
/// Returns the final marker (`None` when the move is finished and cleared).
pub fn run(ctx: &mut RunCtx, mut marker: MoveMarker) -> Result<Option<MoveMarker>, String> {
    if (ctx.game_running)() {
        return Err("Close Farming Simulator 25 first — Silo won't move mods while the game is using them.".into());
    }
    let _ = take_request();
    if marker.state == MoveState::Paused {
        marker.state = MoveState::Running;
    }
    marker.pause_reason = None;
    marker.errors.clear();
    save_marker(ctx.conn, ctx.mods_key, &marker)?;

    'restart: loop {
        let (from, to) = match marker.state {
            MoveState::Cancelling => (marker.to.clone(), marker.from.clone()),
            _ => (marker.from.clone(), marker.to.clone()),
        };
        clean_temps(&to);
        let layout = Layout { mods: ctx.mods.to_path_buf(), archive: to.clone() };
        let allow_symlink = organize::supports_symlinks(&layout);
        let mut rows = db::load_organized(ctx.conn, ctx.mods_key);
        rows.sort_by(|a, b| a.tech_name.cmp(&b.tech_name));
        let sizes: Vec<u64> = rows
            .iter()
            .map(|r| {
                let s = organize::archive_path(&from, &r.category, &r.file_name);
                let d = organize::archive_path(&to, &r.category, &r.file_name);
                size_of(if s.exists() { &s } else { &d })
            })
            .collect();
        let mut p = Progress {
            total: rows.len(),
            bytes_total: sizes.iter().sum(),
            cancelling: marker.state == MoveState::Cancelling,
            ..Default::default()
        };
        for (row, size) in rows.iter().zip(sizes) {
            if (ctx.game_running)() {
                marker.state = MoveState::Paused;
                marker.pause_reason = Some("Farming Simulator 25 started".into());
                save_marker(ctx.conn, ctx.mods_key, &marker)?;
                return Ok(Some(marker));
            }
            match take_request() {
                1 => {
                    marker.state = MoveState::Paused;
                    save_marker(ctx.conn, ctx.mods_key, &marker)?;
                    return Ok(Some(marker));
                }
                2 if marker.state != MoveState::Cancelling => {
                    marker.state = MoveState::Cancelling;
                    save_marker(ctx.conn, ctx.mods_key, &marker)?;
                    continue 'restart;
                }
                _ => {}
            }
            p.current = row.tech_name.clone();
            (ctx.progress)(&p);
            match move_one(row, ctx.mods, &from, &to, allow_symlink, &requested) {
                Ok(_) => {}
                Err(_) if requested() => {
                    // Interrupted mid-copy by pause/cancel: handled at the next boundary.
                    continue;
                }
                Err(e) => marker.errors.push(format!("{}: {e}", row.tech_name)),
            }
            p.done += 1;
            p.bytes_done += size;
            (ctx.progress)(&p);
        }
        // A request that arrived during the last mod.
        match take_request() {
            1 => {
                marker.state = MoveState::Paused;
                save_marker(ctx.conn, ctx.mods_key, &marker)?;
                return Ok(Some(marker));
            }
            2 if marker.state != MoveState::Cancelling => {
                marker.state = MoveState::Cancelling;
                save_marker(ctx.conn, ctx.mods_key, &marker)?;
                continue 'restart;
            }
            _ => {}
        }
        if !marker.errors.is_empty() {
            marker.state = MoveState::Paused;
            save_marker(ctx.conn, ctx.mods_key, &marker)?;
            return Ok(Some(marker));
        }
        // Finished: switch the setting (only on a completed forward move), tidy the folder we
        // left, clear the marker.
        let default = Layout::default_for(ctx.mods).archive;
        if marker.state != MoveState::Cancelling {
            let value = if marker.to == default { None } else { Some(marker.to.to_string_lossy().into_owned()) };
            db::set_app_setting(ctx.conn, &archive_setting_key(ctx.mods_key), value.as_deref())?;
        }
        let _ = organize::cleanup_empty_archive(&from, from == default);
        clear_marker(ctx.conn, ctx.mods_key)?;
        return Ok(None);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub mod_count: usize,
    pub bytes: u64,
    pub free_bytes: Option<u64>,
    pub same_drive: bool,
    pub symlinks_available: bool,
    pub loose_mods: usize,
    pub problems: Vec<String>,
}

/// Path sanity for a new archive location. Pure apart from `same` (inject `modsroot::same_folder`).
pub fn path_problems(
    current: &Path,
    new: &Path,
    mods: &Path,
    other_archives: &[PathBuf],
    same: impl Fn(&Path, &Path) -> bool,
) -> Vec<String> {
    let mut v = Vec::new();
    if same(current, new) {
        v.push("That's already where your library archive is.".into());
    }
    let default = mods.join("archive");
    if new.starts_with(mods) && !same(new, &default) {
        v.push("Pick a folder outside your mods folder (or the default archive).".into());
    }
    if other_archives.iter().any(|a| same(a, new)) {
        v.push("Another mods folder already uses that archive.".into());
    }
    v
}

/// Space problem, if the target drive can't hold the archive plus a 1 GB margin.
pub fn space_problem(bytes: u64, free: Option<u64>) -> Option<String> {
    const MARGIN: u64 = 1 << 30;
    match free {
        Some(f) if f < bytes + MARGIN => Some(format!(
            "Not enough free space: needs {:.1} GB, {:.1} GB free.",
            (bytes + MARGIN) as f64 / 1e9,
            f as f64 / 1e9
        )),
        _ => None,
    }
}

/// Loose mods (zips / mod folders) at the top level of an archive folder — candidates to adopt.
pub fn loose_mods(archive: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(archive)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false)
                        || p.join("modDesc.xml").is_file()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fx {
        base: PathBuf,
        mods: PathBuf,
        old: PathBuf,
        new: PathBuf,
        conn: Connection,
        key: String,
    }

    fn fx(tag: &str) -> Fx {
        let base = std::env::temp_dir().join(format!("silo_reloc_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let old = mods.join("archive");
        let new = base.join("hdd");
        std::fs::create_dir_all(old.join("Tools")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&mods);
        Fx { base, mods, old, new, conn, key }
    }

    fn add_zip(f: &Fx, tech: &str, active: bool) -> OrganizedRow {
        let file = format!("{tech}.zip");
        std::fs::write(f.old.join("Tools").join(&file), format!("PK\x03\x04 {tech}")).unwrap();
        let row = OrganizedRow {
            tech_name: tech.into(),
            file_name: file,
            kind: "zip".into(),
            category: "Tools".into(),
            subcategory: None,
            active,
        };
        db::upsert_organized(&f.conn, &f.key, &row).unwrap();
        row
    }

    fn marker(f: &Fx) -> MoveMarker {
        MoveMarker { from: f.old.clone(), to: f.new.clone(), state: MoveState::Running, errors: vec![], pause_reason: None }
    }

    fn run_with(f: &Fx, m: MoveMarker, game: &dyn Fn() -> bool) -> Result<Option<MoveMarker>, String> {
        let mut prog = |_: &Progress| {};
        let mut ctx = RunCtx { conn: &f.conn, mods: &f.mods, mods_key: &f.key, game_running: game, progress: &mut prog };
        run(&mut ctx, m)
    }

    #[test]
    fn move_one_handles_every_state() {
        let f = fx("states");
        let row = add_zip(&f, "FS25_A", false);
        let never = || false;
        // old only → moved
        assert_eq!(move_one(&row, &f.mods, &f.old, &f.new, false, &never).unwrap(), Step::Moved);
        assert!(f.new.join("Tools/FS25_A.zip").exists() && !f.old.join("Tools/FS25_A.zip").exists());
        // new only → already done
        assert_eq!(move_one(&row, &f.mods, &f.old, &f.new, false, &never).unwrap(), Step::AlreadyDone);
        // both, new corrupt → recopied
        std::fs::write(f.old.join("Tools/FS25_A.zip"), b"PK\x03\x04 FS25_A").unwrap();
        std::fs::write(f.new.join("Tools/FS25_A.zip"), b"corrupt").unwrap();
        assert_eq!(move_one(&row, &f.mods, &f.old, &f.new, false, &never).unwrap(), Step::Moved);
        assert_eq!(std::fs::read(f.new.join("Tools/FS25_A.zip")).unwrap(), b"PK\x03\x04 FS25_A");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn a_full_move_switches_the_setting_and_clears_the_marker() {
        let f = fx("full");
        add_zip(&f, "FS25_A", false);
        add_zip(&f, "FS25_B", false);
        let out = run_with(&f, marker(&f), &|| false).unwrap();
        assert!(out.is_none());
        assert!(f.new.join("Tools/FS25_A.zip").exists() && f.new.join("Tools/FS25_B.zip").exists());
        assert_eq!(
            db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).as_deref(),
            Some(f.new.to_string_lossy().as_ref())
        );
        assert!(load_marker(&f.conn, &f.key).is_none());
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn refuses_to_start_while_the_game_runs() {
        let f = fx("game");
        add_zip(&f, "FS25_A", false);
        assert!(run_with(&f, marker(&f), &|| true).is_err());
        assert!(f.old.join("Tools/FS25_A.zip").exists(), "nothing moved");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn pauses_when_the_game_starts_and_resume_finishes() {
        let f = fx("autopause");
        add_zip(&f, "FS25_A", false);
        add_zip(&f, "FS25_B", false);
        // The game "starts" after the first mod.
        let calls = std::cell::Cell::new(0);
        let game = || {
            calls.set(calls.get() + 1);
            calls.get() > 2 // 1: start check, 2: before FS25_A, 3: before FS25_B
        };
        let paused = run_with(&f, marker(&f), &game).unwrap().unwrap();
        assert_eq!(paused.state, MoveState::Paused);
        assert!(paused.pause_reason.is_some());
        assert!(f.new.join("Tools/FS25_A.zip").exists());
        assert!(f.old.join("Tools/FS25_B.zip").exists(), "B not moved yet");
        // Resume with the game closed.
        assert!(run_with(&f, paused, &|| false).unwrap().is_none());
        assert!(f.new.join("Tools/FS25_B.zip").exists());
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn cancel_moves_everything_back() {
        let f = fx("cancel");
        add_zip(&f, "FS25_A", false);
        add_zip(&f, "FS25_B", false);
        // Move A by hand to simulate a half-done move, then cancel.
        let rows = db::load_organized(&f.conn, &f.key);
        move_one(&rows[0], &f.mods, &f.old, &f.new, false, &|| false).unwrap();
        let mut m = marker(&f);
        m.state = MoveState::Cancelling;
        assert!(run_with(&f, m, &|| false).unwrap().is_none());
        assert!(f.old.join("Tools/FS25_A.zip").exists() && f.old.join("Tools/FS25_B.zip").exists());
        assert!(!f.new.join("Tools/FS25_A.zip").exists());
        assert!(db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).is_none(), "setting unchanged");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn a_failed_mod_keeps_its_original_and_blocks_the_switch() {
        let f = fx("fail");
        add_zip(&f, "FS25_A", false);
        let ghost = OrganizedRow {
            tech_name: "FS25_Ghost".into(),
            file_name: "FS25_Ghost.zip".into(),
            kind: "zip".into(),
            category: "Tools".into(),
            subcategory: None,
            active: false,
        };
        db::upsert_organized(&f.conn, &f.key, &ghost).unwrap(); // no file anywhere
        let out = run_with(&f, marker(&f), &|| false).unwrap().unwrap();
        assert_eq!(out.state, MoveState::Paused);
        assert_eq!(out.errors.len(), 1);
        assert!(db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).is_none());
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn an_active_hardlinked_zip_stays_readable_after_its_old_copy_is_removed() {
        let f = fx("hardlink");
        let row = add_zip(&f, "FS25_A", true);
        let flat = f.mods.join("FS25_A.zip");
        std::fs::hard_link(f.old.join("Tools/FS25_A.zip"), &flat).unwrap();
        move_one(&row, &f.mods, &f.old, &f.new, false, &|| false).unwrap();
        assert_eq!(std::fs::read(&flat).unwrap(), b"PK\x03\x04 FS25_A");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[cfg(unix)]
    #[test]
    fn an_active_symlinked_folder_mod_is_repointed() {
        let f = fx("dirlink");
        let src = f.old.join("Tools/FS25_D");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let row = OrganizedRow {
            tech_name: "FS25_D".into(),
            file_name: "FS25_D".into(),
            kind: "dir".into(),
            category: "Tools".into(),
            subcategory: None,
            active: true,
        };
        db::upsert_organized(&f.conn, &f.key, &row).unwrap();
        std::os::unix::fs::symlink(&src, f.mods.join("FS25_D")).unwrap();
        move_one(&row, &f.mods, &f.old, &f.new, true, &|| false).unwrap();
        assert_eq!(
            std::fs::canonicalize(f.mods.join("FS25_D")).unwrap(),
            std::fs::canonicalize(f.new.join("Tools/FS25_D")).unwrap()
        );
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn path_and_space_checks() {
        let same = |a: &Path, b: &Path| a == b;
        let mods = Path::new("/m");
        assert!(!path_problems(Path::new("/m/archive"), Path::new("/m/archive"), mods, &[], same).is_empty());
        assert!(!path_problems(Path::new("/m/archive"), Path::new("/m/sub"), mods, &[], same).is_empty());
        assert!(!path_problems(Path::new("/m/archive"), Path::new("/x"), mods, &[PathBuf::from("/x")], same).is_empty());
        assert!(path_problems(Path::new("/m/archive"), Path::new("/hdd/a"), mods, &[], same).is_empty());
        assert!(space_problem(10 << 30, Some(5 << 30)).is_some());
        assert!(space_problem(1 << 30, Some(5 << 30)).is_none());
        assert!(space_problem(1 << 30, None).is_none());
    }

    #[test]
    fn finds_loose_mods_at_the_archive_top_level_only() {
        let f = fx("loose");
        std::fs::write(f.new.join("FS25_X.zip"), b"PK").unwrap();
        std::fs::create_dir_all(f.new.join("FS25_Y")).unwrap();
        std::fs::write(f.new.join("FS25_Y/modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::create_dir_all(f.new.join("Tools")).unwrap(); // a category folder, not a mod
        let mut found: Vec<_> = loose_mods(&f.new).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        found.sort();
        assert_eq!(found, vec!["FS25_X.zip", "FS25_Y"]);
        let _ = std::fs::remove_dir_all(&f.base);
    }
}
```

- [ ] **Step 2: Register the module** — `pub mod relocate;` in `lib.rs`'s module list (between `pub mod provenance;` and `pub mod savegame;`).

- [ ] **Step 3: Run.** `cargo test relocate` (after Task 8 makes `lib.rs` compile — or temporarily comment the failing `lib.rs` lines). Expected: `10 passed` (9 on Windows, where the unix-only symlinked-folder test is skipped).

---

### Task 8: `lib.rs` — resolve the active layout, guard writes, scan archives

**Files:**
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Resolve the layout in `compute_mods_root_status`.** After `let available = …;` replace `modsroot::set_active(if available { r.path.clone() } else { None });` with:

```rust
    let active = if available {
        r.path.as_ref().map(|p| {
            let key = modsroot::canonical_key(p);
            let marker = relocate::load_marker(conn, &key);
            let archive = match &marker {
                Some(m) => m.from.clone(),
                None => db::get_app_setting(conn, &relocate::archive_setting_key(&key))
                    .map(PathBuf::from)
                    .unwrap_or_else(|| organize::Layout::default_for(p).archive),
            };
            modsroot::Active {
                layout: organize::Layout { mods: p.clone(), archive },
                move_to: marker.as_ref().map(|m| m.to.clone()),
                move_running: false,
            }
        })
    } else {
        None
    };
    modsroot::set_active(active);
```

- [ ] **Step 2: Trusted roots and the write guard.** Replace `allowed_roots()` and add helpers:

```rust
fn allowed_roots() -> Vec<PathBuf> {
    match modsroot::active() {
        Some(a) => {
            let mut v = vec![a.layout.mods, a.layout.archive];
            v.extend(a.move_to);
            v
        }
        None => fsgame::default_mods_paths(),
    }
}

/// The active layout (mods folder + archive), validated against a frontend-supplied root.
fn primary_layout(root: Option<String>) -> Result<organize::Layout, String> {
    let mods = primary_root(root)?;
    Ok(match modsroot::active() {
        Some(a) if modsroot::same_folder(&a.layout.mods, &mods) => a.layout,
        _ => organize::Layout::default_for(&mods),
    })
}

/// Refuse writes to the library while an archive move is unfinished.
fn ensure_no_move() -> Result<(), String> {
    match modsroot::active() {
        Some(a) if a.move_to.is_some() => Err("Finish or cancel the archive move first.".into()),
        _ => Ok(()),
    }
}

/// Refuse organize writes while a custom archive's drive is missing (nothing is deleted; the
/// banner offers Retry / Choose another).
fn ensure_archive_available() -> Result<(), String> {
    match modsroot::active() {
        Some(a) if !a.layout.archive_is_default() && !a.layout.archive.is_dir() => Err(
            "Your library archive isn't available — reconnect its drive or choose another.".into(),
        ),
        _ => Ok(()),
    }
}
```

`primary_root` must only accept a **mods** folder: change its `let allowed = allowed_roots();` to `let allowed: Vec<PathBuf> = match modsroot::active() { Some(a) => vec![a.layout.mods], None => fsgame::default_mods_paths() };`.

- [ ] **Step 3: Organize call sites.** Replace Task 2's temporary `organize::Layout::default_for(&root)` wrappers: in each command, `let root = primary_root(root)?;` becomes `let layout = primary_layout(root)?;` and the engine call takes `&layout` (for `apply_organize` also pass `&layout.mods` as `source`). Add `ensure_no_move()?;` as the first line of: `apply_organize`, `adopt_foreign_file`, `restore_foreign_file`, `set_active`, `relink_projections`, `flatten`, `install_remote_mod`, `download_update`, `generate_bridge`, `collection_apply`, `set_mods_root`, `clear_mods_root`, `point_game_at_mods_root`. `plan_organize`, `hardlink_support`, `detect_foreign_files` are read-only and stay unguarded. Also add `ensure_archive_available()?;` right after `ensure_no_move()?;` in `apply_organize`, `adopt_foreign_file`, `restore_foreign_file`, `set_active`, `relink_projections`, `flatten` (not in the folder-switch commands, so a user can still switch away from a folder whose archive drive is gone).

- [ ] **Step 4: Launch guard.**

```rust
#[tauri::command]
async fn launch_game() -> Result<(), String> {
    if modsroot::active().map(|a| a.move_running).unwrap_or(false) {
        return Err("Silo is moving your library archive — pause the move to play.".into());
    }
    tauri::async_runtime::spawn_blocking(gamelaunch::launch)
        .await
        .map_err(|e| e.to_string())?
}
```

- [ ] **Step 5: Scan with the right archives.** In `scan_mods`, after `let roots: Vec<PathBuf> = …;` build layouts and call `scan_layouts`:

```rust
    let layouts: Vec<(PathBuf, Vec<PathBuf>)> = roots
        .iter()
        .map(|r| match modsroot::active() {
            Some(a) if modsroot::same_folder(&a.layout.mods, r) => {
                // During a move the destination is read first (it wins for moved mods).
                let mut archives: Vec<PathBuf> = a.move_to.into_iter().collect();
                archives.push(a.layout.archive);
                (r.clone(), archives)
            }
            _ => (r.clone(), vec![organize::Layout::default_for(r).archive]),
        })
        .collect();
```

and replace `scan::scan_cached(roots, &cache, …)` with `scan::scan_layouts(layouts, &cache, …)`.

- [ ] **Step 6: Run the whole suite.** `cargo test` → all pass (including `relocate` and `modsroot`); `cargo clippy --all-targets -- -D warnings` → clean.

- [ ] **Step 7: Commit Tasks 6-8.**

```bash
git add src-tauri/src/modsroot.rs src-tauri/src/relocate.rs src-tauri/src/lib.rs
git commit -m "feat: archive move job (relocate) + active layout, write guard, two-archive scan"
```

---

### Task 9: Archive commands

**Files:**
- Modify: `src-tauri/src/lib.rs` (commands + handler registration)

- [ ] **Step 1: Add the commands** after the mods-folder section:

```rust
// ── Library archive location (#166) ──

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveStatus {
    path: Option<String>,
    is_default: bool,
    available: bool,
    moving: Option<relocate::MoveMarker>,
}

fn active_mods_key() -> Result<(organize::Layout, String), String> {
    let a = modsroot::active().ok_or_else(|| "No mods folder selected".to_string())?;
    let key = modsroot::canonical_key(&a.layout.mods);
    Ok((a.layout, key))
}

#[tauri::command]
async fn archive_status(app: tauri::AppHandle) -> Result<ArchiveStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let Some(a) = modsroot::active() else {
            return Ok(ArchiveStatus { path: None, is_default: true, available: false, moving: None });
        };
        let conn = db::open(&db)?;
        let key = modsroot::canonical_key(&a.layout.mods);
        Ok(ArchiveStatus {
            path: Some(a.layout.archive.to_string_lossy().into_owned()),
            is_default: a.layout.archive_is_default(),
            available: a.layout.archive.is_dir() || a.layout.archive_is_default(),
            moving: relocate::load_marker(&conn, &key),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn archive_move_preflight(app: tauri::AppHandle, path: String) -> Result<relocate::Preflight, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (layout, key) = active_mods_key()?;
        let new = PathBuf::from(&path);
        modsroot::validate_folder(&new)?;
        let conn = db::open(&db)?;
        let rows = db::load_organized(&conn, &key);
        let bytes: u64 = rows
            .iter()
            .map(|r| {
                let p = organize::archive_path(&layout.archive, &r.category, &r.file_name);
                std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0)
            })
            .sum();
        // Archives already used by other mods folders Silo knows (#176's known list).
        let others: Vec<PathBuf> = modsroot::parse_known(db::get_app_setting(&conn, modsroot::KNOWN_KEY).as_deref())
            .into_iter()
            .map(PathBuf::from)
            .filter(|p| !modsroot::same_folder(p, &layout.mods))
            .filter_map(|p| db::get_app_setting(&conn, &relocate::archive_setting_key(&modsroot::canonical_key(&p))))
            .map(PathBuf::from)
            .collect();
        let mut problems = relocate::path_problems(&layout.archive, &new, &layout.mods, &others, modsroot::same_folder);
        let free = organize::mount_of(&new).map(|(_, f)| f);
        let same_drive = organize::same_volume(&layout.mods, &new);
        if !same_drive {
            problems.extend(relocate::space_problem(bytes, free));
        }
        let target = organize::Layout { mods: layout.mods.clone(), archive: new.clone() };
        Ok(relocate::Preflight {
            mod_count: rows.len(),
            bytes,
            free_bytes: free,
            same_drive,
            symlinks_available: organize::supports_symlinks(&target),
            loose_mods: relocate::loose_mods(&new).len(),
            problems,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
```

For folder mods `std::fs::metadata(dir).len()` undercounts; acceptable for the dialog estimate (the run's progress uses real sizes).

- [ ] **Step 2: Start / resume / pause / cancel / default.**

```rust
/// Run the move until it finishes or pauses; emits `archive-move-progress`.
fn drive_move(app: &tauri::AppHandle, db: &Path, marker: relocate::MoveMarker) -> Result<Option<relocate::MoveMarker>, String> {
    let (layout, key) = active_mods_key()?;
    let conn = db::open(db)?;
    let mut active = modsroot::active().ok_or("No mods folder selected")?;
    active.move_to = Some(marker.to.clone());
    active.move_running = true;
    modsroot::set_active(Some(active));
    let emitter = app.clone();
    let mut prog = move |p: &relocate::Progress| {
        let _ = emitter.emit("archive-move-progress", p.clone());
    };
    let game = gamelaunch::fs25_running;
    let mut ctx = relocate::RunCtx {
        conn: &conn,
        mods: &layout.mods,
        mods_key: &key,
        game_running: &game,
        progress: &mut prog,
    };
    let out = relocate::run(&mut ctx, marker);
    // Re-resolve the layout (setting switched, marker paused or cleared).
    let _ = compute_mods_root_status(&conn);
    out
}

#[tauri::command]
async fn archive_move_start(app: tauri::AppHandle, path: String) -> Result<Option<relocate::MoveMarker>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        ensure_no_move()?;
        let (layout, key) = active_mods_key()?;
        let conn = db::open(&db)?;
        let marker = relocate::MoveMarker {
            from: layout.archive.clone(),
            to: PathBuf::from(&path),
            state: relocate::MoveState::Running,
            errors: vec![],
            pause_reason: None,
        };
        if gamelaunch::fs25_running() {
            return Err("Close Farming Simulator 25 first — Silo won't move mods while the game is using them.".into());
        }
        relocate::save_marker(&conn, &key, &marker)?;
        drive_move(&app, &db, marker)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn archive_move_resume(app: tauri::AppHandle) -> Result<Option<relocate::MoveMarker>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, key) = active_mods_key()?;
        let conn = db::open(&db)?;
        let m = relocate::load_marker(&conn, &key).ok_or("No archive move to resume")?;
        drive_move(&app, &db, m)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn archive_move_pause() {
    relocate::request_pause();
}

/// Cancel: if the move is running, the loop switches direction; if paused, run it in reverse.
#[tauri::command]
async fn archive_move_cancel(app: tauri::AppHandle) -> Result<Option<relocate::MoveMarker>, String> {
    if modsroot::active().map(|a| a.move_running).unwrap_or(false) {
        relocate::request_cancel();
        return Ok(None);
    }
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (_, key) = active_mods_key()?;
        let conn = db::open(&db)?;
        let mut m = relocate::load_marker(&conn, &key).ok_or("No archive move to cancel")?;
        m.state = relocate::MoveState::Cancelling;
        relocate::save_marker(&conn, &key, &m)?;
        drive_move(&app, &db, m)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move the archive back to `<mods>/archive`.
#[tauri::command]
async fn archive_use_default(app: tauri::AppHandle) -> Result<Option<relocate::MoveMarker>, String> {
    let default = active_mods_key()?.0.mods.join("archive");
    std::fs::create_dir_all(&default).map_err(|e| e.to_string())?;
    archive_move_start(app, default.to_string_lossy().into_owned()).await
}
```

`relocate::run` itself also refuses to start while the game runs; the check in `archive_move_start` gives the message before the marker is written.

- [ ] **Step 3: Adopt preview / apply.**

```rust
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdoptPreview {
    plan: Vec<organize::PlannedMove>,
    inputs: Vec<organize::ModInput>,
    duplicates: Vec<String>,
}

#[tauri::command]
async fn archive_adopt_preview(app: tauri::AppHandle) -> Result<AdoptPreview, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (layout, key) = active_mods_key()?;
        let conn = db::open(&db)?;
        let known: std::collections::HashSet<String> =
            db::load_organized(&conn, &key).into_iter().map(|r| r.tech_name).collect();
        // Scan the archive's loose top level as if it were a mods folder.
        let found = scan::scan_layouts(vec![(layout.archive.clone(), vec![])], &Default::default(), |_, _| {});
        let mut inputs = Vec::new();
        let mut duplicates = Vec::new();
        for m in found.result.mods {
            if known.contains(&m.tech_name) {
                duplicates.push(m.tech_name);
                continue;
            }
            let file_name = std::path::Path::new(&m.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            inputs.push(organize::ModInput {
                tech_name: m.tech_name,
                file_name,
                kind: m.kind,
                category: m.category,
                subcategory: m.subcategory,
            });
        }
        Ok(AdoptPreview { plan: organize::plan_organize(&layout.archive, &inputs), inputs, duplicates })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn archive_adopt_apply(app: tauri::AppHandle, mods: Vec<organize::ModInput>) -> Result<organize::Report, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        ensure_no_move()?;
        let (layout, _) = active_mods_key()?;
        let conn = db::open(&db)?;
        Ok(organize::apply_organize(&conn, &layout, &layout.archive.clone(), &mods))
    })
    .await
    .map_err(|e| e.to_string())?
}
```

`ModInput` needs `Serialize` (it derives only `Deserialize`) — add `Serialize` to its derive in `organize.rs`.

- [ ] **Step 4: Register** in `generate_handler![…]`: `archive_status, archive_move_preflight, archive_move_start, archive_move_resume, archive_move_pause, archive_move_cancel, archive_use_default, archive_adopt_preview, archive_adopt_apply,`.

- [ ] **Step 5: Gate.** `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test` → clean.

- [ ] **Step 6: Commit.**

```bash
git add src-tauri/src/lib.rs src-tauri/src/organize.rs
git commit -m "feat: archive location commands (status, preflight, move/pause/resume/cancel, adopt)"
```

---

### Task 10: Frontend API

**Files:**
- Modify: `src/lib/api.ts`

- [ ] **Step 1: Add** after the mods-root wrappers:

```ts
export type MoveState = "running" | "paused" | "cancelling";

export interface MoveMarker {
  from: string;
  to: string;
  state: MoveState;
  errors: string[];
  pauseReason: string | null;
}

export interface ArchiveStatus {
  path: string | null;
  isDefault: boolean;
  available: boolean;
  moving: MoveMarker | null;
}

export interface ArchivePreflight {
  modCount: number;
  bytes: number;
  freeBytes: number | null;
  sameDrive: boolean;
  symlinksAvailable: boolean;
  looseMods: number;
  problems: string[];
}

export interface MoveProgress {
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
  current: string;
  cancelling: boolean;
}

export interface AdoptPreview {
  plan: PlannedMove[];
  inputs: ModInput[];
  duplicates: string[];
}

export const archiveStatus = () => invoke<ArchiveStatus>("archive_status");
export const archiveMovePreflight = (path: string) =>
  invoke<ArchivePreflight>("archive_move_preflight", { path });
/** Resolves when the move finishes (null) or pauses (the marker). */
export const archiveMoveStart = (path: string) =>
  invoke<MoveMarker | null>("archive_move_start", { path });
export const archiveMoveResume = () => invoke<MoveMarker | null>("archive_move_resume");
export const archiveMovePause = () => invoke<void>("archive_move_pause");
export const archiveMoveCancel = () => invoke<MoveMarker | null>("archive_move_cancel");
export const archiveUseDefault = () => invoke<MoveMarker | null>("archive_use_default");
export const archiveAdoptPreview = () => invoke<AdoptPreview>("archive_adopt_preview");
export const archiveAdoptApply = (mods: ModInput[]) =>
  invoke<OrganizeReport>("archive_adopt_apply", { mods });
export function onMoveProgress(cb: (p: MoveProgress) => void): Promise<UnlistenFn> {
  return listen<MoveProgress>("archive-move-progress", (e) => cb(e.payload));
}
```

(`PlannedMove`, `ModInput`, `OrganizeReport` are the existing types used by `planOrganize`/`applyOrganize` — import them from `./types` if not already in scope.)

- [ ] **Step 2:** `npm run check` → 0 errors. Commit:

```bash
git add src/lib/api.ts
git commit -m "feat(ui): archive location API wrappers"
```

---

### Task 11: Settings section, banners, move controls, adopt, launch block

**Files:**
- Modify: `src/lib/components/panels/SettingsPanel.svelte`
- Modify: `src/lib/components/panels/Topbar.svelte`
- Modify: `src/App.svelte`

- [ ] **Step 1: Topbar** — add prop `launchBlocked: boolean` (destructure + type) and change the Launch button's `disabled={!!busy}` to `disabled={!!busy || launchBlocked}`.

- [ ] **Step 2: SettingsPanel** — add props `archive: ArchiveStatus | null`, `onChangeArchive: () => void`, `onArchiveDefault: () => void` (import `type ArchiveStatus` from `../../api`), and below the "Mods folder" section:

```svelte
  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Library archive</div>
      {#if archive?.path}
        <button class="set-link" onclick={() => archive?.path && openFolder(archive.path).catch(() => {})}>
          Open ↗
        </button>
      {/if}
      <button class="set-link" disabled={!!busy || scanning || !!archive?.moving} onclick={onChangeArchive}>
        Change…
      </button>
    </div>
    {#if archive?.path}
      <div class="set-path" class:muted={!archive.available}>{archive.path}</div>
      <div class="set-hint">
        {archive.isDefault ? "Inside your mods folder (default)" : "Custom location"}{archive.available
          ? ""
          : " · not available"}
      </div>
      {#if !archive.isDefault}
        <button class="set-link" disabled={!!busy || scanning || !!archive.moving} onclick={onArchiveDefault}>
          Use default
        </button>
      {/if}
    {/if}
  </div>
```

- [ ] **Step 3: App state and helpers.** Import `archiveStatus, archiveMovePreflight, archiveMoveStart, archiveMoveResume, archiveMovePause, archiveMoveCancel, archiveUseDefault, archiveAdoptPreview, archiveAdoptApply, onMoveProgress, type ArchiveStatus, type MoveProgress`. Add state:

```ts
  let archive = $state<ArchiveStatus | null>(null);
  let moveProgress = $state<MoveProgress | null>(null);
  let moveRunning = $state(false);
```

In startup, after `modsStatus = await modsRootStatus();` add `archive = await archiveStatus();` and register `onMoveProgress((p) => (moveProgress = p))` (store the unlisten with the other unlisteners and call it in the cleanup). In `applyModsStatus`, after `modsStatus = st;`, add `archive = await archiveStatus().catch(() => null);`.

```ts
  const gb = (b: number) => (b / 1e9).toFixed(1);

  /** Run a move-ish invoke, then refresh everything that depends on where mods live. */
  async function driveMove(call: () => Promise<unknown>) {
    moveRunning = true;
    try {
      await call();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      moveRunning = false;
      moveProgress = null;
      archive = await archiveStatus().catch(() => null);
      modsStatus = await modsRootStatus().catch(() => modsStatus);
      await runScan();
      if (!archive?.moving) await offerAdopt();
    }
  }

  async function changeArchive() {
    const path = await pickFolder();
    if (!path) return;
    try {
      const pf = await archiveMovePreflight(path);
      if (pf.problems.length) {
        errorMsg = pf.problems.join(" ");
        return;
      }
      let msg =
        `Move your library (${pf.modCount} mods, ${gb(pf.bytes)} GB) to ${path}?\n\n` +
        "Active mods stay playable — Silo links them into your mods folder" +
        (pf.sameDrive ? "." : " (or copies them if links aren't available).");
      if (!pf.sameDrive && pf.freeBytes != null) msg += `\n\n${gb(pf.freeBytes)} GB free on that drive.`;
      if (!pf.sameDrive && !pf.symlinksAvailable)
        msg += "\n\nTurn on Windows Developer Mode to link active mods instead of copying them.";
      msg += "\n\nClose Farming Simulator 25 first. You can pause the move to play and resume later.";
      if (!(await confirmDialog(msg, { okLabel: "Move library", cancelLabel: "Cancel" }))) return;
      await driveMove(() => archiveMoveStart(path));
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function archiveDefault() {
    if (
      !(await confirmDialog("Move your library back into your mods folder?", {
        okLabel: "Move library",
        cancelLabel: "Cancel",
      }))
    )
      return;
    await driveMove(() => archiveUseDefault());
  }

  const pauseMove = () => archiveMovePause().catch((e) => (errorMsg = String(e)));
  const resumeMove = () => driveMove(() => archiveMoveResume());
  async function cancelMove() {
    if (
      !(await confirmDialog("Cancel the move? Mods already moved go back to where they were.", {
        okLabel: "Cancel move",
        cancelLabel: "Keep going",
      }))
    )
      return;
    if (moveRunning) await archiveMoveCancel();
    else await driveMove(() => archiveMoveCancel());
  }

  /** After a move, if the archive folder holds loose mods, offer to file them in. */
  async function offerAdopt() {
    const pv = await archiveAdoptPreview().catch(() => null);
    if (!pv || pv.inputs.length === 0) return;
    organizePreview = {
      inputs: pv.inputs,
      techNames: pv.inputs.map((m) => m.techName),
      plan: pv.plan,
      loading: false,
      applying: false,
      adopt: true,
    };
    if (pv.duplicates.length)
      errorMsg = `${pv.duplicates.length} mod(s) in that folder are already in your library — left untouched.`;
  }
```

- [ ] **Step 4: Route the preview's confirm for adoption.** Add `adopt?: boolean` to the `organizePreview` state type. In `confirmOrganize`, at the top after the guard:

```ts
    if (organizePreview.adopt) {
      organizePreview = { ...organizePreview, applying: true };
      try {
        await archiveAdoptApply(organizePreview.inputs);
      } catch (e) {
        errorMsg = String(e);
      }
      organizePreview = null;
      await runScan();
      return;
    }
```

- [ ] **Step 5: Banners** — after the mods-folder banners:

```svelte
  {#if archive?.moving || moveRunning}
    <div class="cloud-notice">
      <span class="cn-ico">⇄</span>
      <div class="cn-text">
        {#if moveRunning && moveProgress}
          <b>{moveProgress.cancelling ? "Moving your library back" : "Moving your library"}</b> —
          {moveProgress.done}/{moveProgress.total} mods · {gb(moveProgress.bytesDone)}/{gb(moveProgress.bytesTotal)} GB
        {:else if archive?.moving?.errors.length}
          <b>{archive.moving.errors.length} mod(s) couldn't be moved.</b> {archive.moving.errors.join("; ")}
        {:else if archive?.moving}
          <b>Archive move paused.</b>
          {archive.moving.pauseReason ? `${archive.moving.pauseReason}. ` : ""}You can play — resume when you're done.
        {/if}
      </div>
      {#if moveRunning}
        <button class="cn-act" onclick={pauseMove}>Pause</button>
      {:else if archive?.moving}
        <button class="cn-act" onclick={resumeMove}>{archive.moving.errors.length ? "Retry" : "Resume"}</button>
      {/if}
      <button class="cn-act" onclick={cancelMove}>Cancel move</button>
    </div>
  {:else if archive && !archive.available}
    <div class="cloud-notice">
      <span class="cn-ico">⚠</span>
      <div class="cn-text">
        <b>Your library archive isn't available:</b> <span class="tnum">{archive.path}</span>. Parked mods
        are hidden until it's back; active mods linked from it won't load in-game.
      </div>
      <button class="cn-act" onclick={async () => { archive = await archiveStatus(); await runScan(); }}>Retry</button>
      <button class="cn-act" onclick={changeArchive}>Choose another</button>
    </div>
  {/if}
```

- [ ] **Step 6: Wire props.** `<SettingsPanel … {archive} onChangeArchive={changeArchive} onArchiveDefault={archiveDefault} />`; `<Topbar … launchBlocked={moveRunning} />`. Add `archive?.moving` to `switchBlockReason()`: `if (archive?.moving || moveRunning) return "Finish or cancel the archive move first.";`.

- [ ] **Step 7: Build.** `npm run build` → guard passes, 0 svelte-check errors. Commit:

```bash
git add src/App.svelte src/lib/components/panels/SettingsPanel.svelte src/lib/components/panels/Topbar.svelte
git commit -m "feat(ui): library archive setting, move banner with pause/resume/cancel, adopt preview"
```

---

### Task 12: Docs

**Files:** `CHANGELOG.md`, `docs/SMOKE_TEST.md`, `docs/CROSS-PLATFORM.md`, `CLAUDE.md`, `docs/superpowers/specs/2026-10-16-archive-location-design.md`

- [ ] **Step 1: CHANGELOG** under `[Unreleased]`:

```markdown
### Added

- **Put your library archive on any drive.** Settings → Library archive → **Change…** moves your
  parked mods (for example to a big HDD) while your mods folder stays where the game reads it. The
  move verifies every mod before removing the original, shows progress, refuses to run while
  Farming Simulator 25 is open, and can be **paused** — play, then resume — or **cancelled**, which
  moves everything back. Pointing it at a folder that already holds mods offers to file them into
  your library. Across drives, active mods are linked (Linux, macOS, Windows with Developer Mode)
  or copied. (#166)
```

- [ ] **Step 2: SMOKE_TEST** — add `## 1c. Library archive location` with: move to another drive on a disposable mods folder + forensic diff; pause → launch FS25 → mods load → Resume refused while FS25 runs → resume after closing; cancel mid-move restores the tree; adopt a folder with loose zips (duplicates untouched); unplug the archive drive → banner, nothing deleted; Windows Developer Mode on → active zips are symlinks and **FS25 loads them** (release gate).

- [ ] **Step 3: CROSS-PLATFORM** — add a "Library archive" paragraph: per-folder location; projection hardlink → symlink → copy; Windows symlinks need Developer Mode; `sysinfo` detects FS25 (`FarmingSimulator2025.exe`, `FarmingSimulator2025Game.exe`, `dedicatedServer.exe`; Proton via command line).

- [ ] **Step 4: CLAUDE.md** — feature bullet "Library archive location (`relocate`, #166)" and add `relocate` to the module list.

- [ ] **Step 5: Spec note** — "Implementation notes" section: `Layout` lives in `organize`, the active-root seam in `modsroot` carries `Active { layout, move_to, move_running }`; `sysinfo` 0.36 (MSRV-aware resolver); free space and same-drive via `sysinfo` disks.

- [ ] **Step 6: Commit.**

```bash
git add CHANGELOG.md docs/SMOKE_TEST.md docs/CROSS-PLATFORM.md CLAUDE.md docs/superpowers/specs/2026-10-16-archive-location-design.md
git commit -m "docs: library archive location — changelog, smoke test, cross-platform, spec notes"
```

---

### Task 13: Verify on a disposable library, then PR

- [ ] **Step 1: Full gate** (fmt, clippy, test, npm build) — green.

- [ ] **Step 2: Disposable mods folder.** Copy ~20 small mod zips from the real library into `<scratch>/silo-test-mods` (copy, never move). Back up `silo.db` (`%APPDATA%/com.hllmr.silo/`) and snapshot `gameSettings.xml`. Build the debug app, switch Silo to the scratch folder (Settings → Mods folder → Change… → "Only in Silo"), Organize, activate 3 mods.

- [ ] **Step 3: Forensic baseline** of the scratch mods folder and its archive (size+mtime CSV per `RC_TESTING.md`), then:
  - Change the archive to a folder on another drive (e.g. `E:\silo-test-archive`) → progress → finishes; diff: every change explained (archive files moved; active zips still readable).
  - Change back via **Use default**; start again and **Pause** midway → banner; launch FS25 (pointed at the scratch folder is optional — confirm Resume is refused while FS25 runs); close FS25 → **Resume** → finishes.
  - Start a move and **Cancel** → tree restored (diff empty apart from timestamps on re-copied files).
  - Drop 2 loose zips (one duplicate) into the archive folder and run a move to it → adopt preview lists 1, duplicate untouched.

- [ ] **Step 4: Restore** — switch Silo back to the game's folder, restore `silo.db` from backup, confirm `gameSettings.xml` MD5 unchanged, delete the scratch folders.

- [ ] **Step 5: Release gate for symlinks** (owner): with Windows Developer Mode on and the archive on another drive, active zips become symlinks — launch FS25 and confirm they load. If they don't, set `supports_symlinks` to return `false` on Windows before merging and note it in the CHANGELOG.

- [ ] **Step 6: PR** `feat: configurable library archive location (#166)` with `Fixes #166`, the verification record, CI green → squash-merge → delete the branch **only after `gh pr view --json state` reads `MERGED`**.
