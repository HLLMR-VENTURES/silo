# Contributing to Silo

Thanks for taking the time to help. Silo is a native desktop **mod manager for
Farming Simulator 25** — a Tauri v2 app with a Rust core and a Svelte 5 frontend.
Issues and pull requests are welcome, whether it's a bug fix, a new panel, a
platform fix, or just a clearer error message.

By participating you agree to abide by our
[Code of Conduct](CODE_OF_CONDUCT.md).

## Dev setup

You need three things:

1. **Rust** (stable, via [rustup](https://rustup.rs)).
2. **Node 22** ([nodejs.org](https://nodejs.org)) — CI builds on Node 22.
3. **Tauri v2 prerequisites** for your OS — the system libraries (WebView, build
   tools, etc.). Follow the official list:
   <https://v2.tauri.app/start/prerequisites/>. On Linux that's `webkit2gtk-4.1`
   and friends; see `.github/workflows/ci.yml` for the exact apt packages we
   install in CI.

Then, from the repo root:

```bash
npm ci                # install frontend deps from the lockfile
npm run tauri:dev     # run the app in dev (hot-reloads the frontend)
npm run tauri:build   # produce an installer in src-tauri/target/release/bundle/
```

The first `tauri:dev` compiles the Rust core, so expect a slow initial build;
subsequent runs are cached.

## Project layout

Silo is one app split across two halves:

- **`src-tauri/src/`** — the Rust core. All the real work lives here as small,
  pure modules behind thin `#[tauri::command]` wrappers: `scan`, `moddesc`,
  `category`, `icons`, `db`, `store`, `fsgame`, `organize`, `savegame`,
  `conflicts`, `xmlconfig`, `gamelaunch`, `github`, `siloapi` (catalog client),
  `logscan` (crash triage), `bisect` (guided bisection), `bindings` (input map),
  `mpsync` (multiplayer manifest), `bridge` (filltype companion generator), and
  more. Each module unit-tests without a running app.
- **`src/`** — the Svelte 5 (runes) + TypeScript + Vite frontend. Panels live in
  `src/lib/components/`. Design tokens are hand-authored CSS; no heavyweight UI
  kit. Long lists are virtualized.
- **`docs/`** — architecture and per-feature notes (`ARCHITECTURE.md`,
  `DESIGN.md`, `CROSS-PLATFORM.md`, `MVP.md`, and more). Read these before a
  large change.

## Working principles

These are the rules the codebase is built on (see [`CLAUDE.md`](CLAUDE.md) for the
full version). PRs are reviewed against them:

- **Off-thread by construction.** If it parses an archive, hashes, decodes an
  image, or walks a tree, it belongs in Rust on a worker thread — never in a
  blocking `invoke` handler, and never in the frontend. Keeping heavy work off
  the UI thread is the number-one performance rule here.
- **Every file write is reversible.** Never touch the user's game files
  destructively without an undo path. Prefer symlink/junction projection over
  moving or copying originals; back up before writing into
  `Documents/My Games/FarmingSimulator2025/`.
- **Parse, don't regex.** `modDesc.xml` and savegame XML go through `quick-xml`,
  not string scraping. Regex XML parsing is a common bug source in this class of app.
- **Small, testable modules.** Keep the scan / library / projection / conflict
  engines as pure logic behind thin Tauri command wrappers, so they unit-test
  without launching the app.
- **Cross-platform, no hardcoded paths.** Windows, macOS, and Linux are all
  supported. Detect capability (e.g. symlink vs. junction vs. copy) and fall
  back gracefully rather than assuming a platform.
- **Cache aggressively, invalidate honestly** — keyed on path + mtime + size.
  Never wipe the whole cache on a version bump.

## Running the checks CI enforces

Before opening a PR, run what CI runs. `.github/workflows/ci.yml` gates `main`: the
frontend job, Rust on Ubuntu (fmt, clippy, test), clippy + test on macOS and Windows, and
`cargo audit`.

**Frontend**

```bash
npm ci
npm test              # Vitest unit and component tests (CI runs this)
npm run check         # svelte-check — type/Svelte diagnostics
npm run build         # what CI actually runs: svelte-check + vite build
```

`npm run build` is the stricter gate — it runs `svelte-check` *and* the
production Vite build, catching type errors and a broken bundle in one step.

**Rust** (from `src-tauri/`)

```bash
cd src-tauri
cargo fmt --all --check   # CI fails on unformatted code — run `cargo fmt` to fix
cargo clippy --all-targets -- -D warnings   # CI fails on any warning
cargo test                # the module unit tests (hundreds of them)
cargo audit               # blocking: CI fails on a vulnerable dependency
```

Please add or update tests when you change core logic — the modules are designed
to be tested without a running app, so there's no excuse not to.

## Commit messages

Match the existing history (`git log`). Keep the subject a short, imperative,
present-tense summary of the change, and scope it with a leading area where it
helps, e.g.:

```
Conflicts: detect cross-mod fillType overrides (SDK-verified true-positive)
fix(category): don't let mission-vehicle store items decide a mod's category
docs: bring CLAUDE.md/ARCHITECTURE current
CI: link libstdc++ on Linux for intel_tex_2's C++ ISPC code
```

Both a freeform `Area: summary` and a conventional-commit `type(scope): summary`
style appear in the log — either is fine. Keep it concise, explain the *why* in
the body if it isn't obvious, and reference an issue (`#123`) when there is one.

## Branch & PR flow

Everything lands through an issue → branch → PR. We don't commit fixes or features
straight to `main`.

1. **Open an issue first** (use the [bug](.github/ISSUE_TEMPLATE/bug_report.md) or
   [feature](.github/ISSUE_TEMPLATE/feature_request.md) template). For anything more
   than a trivial change, agree on the approach in the issue before writing code.
2. **Branch off `main`** with a short descriptive name: `fix/…`, `feat/…`, or
   `docs/…` — e.g. `fix/duplicate-map-crash`, `feat/mod-set-export`.
3. Make focused commits; keep unrelated changes out of the PR.
4. **Update the changelog** — add a line under `## [Unreleased]` in
   [`CHANGELOG.md`](CHANGELOG.md) for any user-facing change (see below).
5. Run the checks above locally — they must pass.
6. **Open a PR** against `HLLMR/silo` `main`. Reference the issue (`Fixes #123`) and
   fill in the [pull request template](.github/pull_request_template.md): what and
   why, tests passing, off-thread/reversible where relevant, docs updated.
7. CI runs the frontend and Rust jobs on every PR; a maintainer reviews against the
   working principles above, then squash-or-merges. Delete the branch after merge.

Small PRs get reviewed faster.

## Changelog

We keep a human-readable [`CHANGELOG.md`](CHANGELOG.md) in the
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. Every user-facing
change belongs under the top **`## [Unreleased]`** heading, in the right group —
`Added`, `Changed`, `Fixed`, `Security`, `Removed`, or `Deprecated`. Write for a
*player*, not a compiler: say what changed on their machine and why it matters, not
which function moved. Internal-only refactors don't need an entry.

## Releases & versioning

Silo follows [Semantic Versioning](https://semver.org). Releases are cut by
maintainers and are gated on the hostile-QA protocol in
[`docs/RC_TESTING.md`](docs/RC_TESTING.md) — read it before proposing a release. In short:

1. **Freeze** a release candidate (record the commit + installer hash) and run the
   RC battery on the packaged build against a *sacrificial* library — never the real one.
2. When it survives (no hard-stop; see the doc's stop-list), on a `release/X.Y.Z` branch
   **bump the version in every source together**: `package.json`, `package-lock.json`
   (two places), `src-tauri/Cargo.toml`, the `silo` entry in `src-tauri/Cargo.lock`, and
   `src-tauri/tauri.conf.json`, and cut `## [Unreleased]` to `## [X.Y.Z] - <date>` in the
   changelog (leaving a fresh empty `[Unreleased]`). Open a PR; merge it on green CI.
3. **Tag `vX.Y.Z`** on that `main` commit once its CI run is green → the release workflow
   (`.github/workflows/release.yml`) first runs a guard that refuses any tag whose commit
   isn't on `main` with every CI check green, then waits for the owner to **approve the
   `release` environment** in the Actions tab (that environment holds the signing keys and
   admits only `v*` tags). After approval it builds and signs the platform installers into a
   **draft, non-prerelease** release with `latest.json`, `SHA256SUMS.txt`, an SBOM and
   build-provenance attestations.
4. **Verify the draft** (embedded version, checksums of the *CI-built* artifacts, a
   clean-machine install, an upgrade from the previous release, release notes stating the
   signing state, known limitations and upgrade guidance), then publish. Publishing makes it
   *Latest*, which is what the in-app updater offers every user straight away.

## Questions

Open an issue on [HLLMR/silo](https://github.com/HLLMR/silo/issues). For security
concerns, follow [SECURITY.md](SECURITY.md) instead of a public issue.
