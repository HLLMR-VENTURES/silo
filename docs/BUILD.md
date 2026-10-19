# Building Silo

## Prerequisites

- **Rust** (stable, MSVC toolchain on Windows), **Node 22** (matches CI), **npm**.
- **Tauri v2 system deps** per OS: https://v2.tauri.app/start/prerequisites/
  - Windows: **MSVC C++ build tools + Windows SDK** and WebView2 runtime.
  - macOS: Xcode command-line tools.
  - Linux: webkit2gtk + build essentials.
- Tauri CLI comes from `devDependencies` (`npm run tauri …`) — no global install.

## Commands

```bash
npm ci                 # frontend deps + Tauri CLI
npm run tauri:dev      # run the app (Vite + Rust, hot reload)
npm run tauri:build    # production bundle
# frontend only:
npm test               # Vitest component and unit tests
npm run build          # guard scripts + svelte-check + vite build  → dist/
# Rust only (from src-tauri/):
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test             # unit tests
cargo run --example scan_real            # scan the default mods folder, print summary
cargo run --example scan_real -- <path>  # scan a specific folder
```

## Windows toolchain

Requires a complete MSVC C++ toolset + Windows SDK (the "Desktop development with
C++" workload). With that installed, plain `cargo build` / `npm run tauri:dev`
work directly — cargo auto-detects MSVC via vswhere; no Developer-Command-Prompt
or `vcvarsall.bat` wrapper needed.

> History: an earlier broken VS install (a toolset with `cl.exe` but no `include/`
> `lib/`) caused `C1083: 'excpt.h' not found` on C deps like `vswhom-sys`. That's
> resolved — the toolchain is fixed and the workaround is no longer required. A cold
> clean build of the whole Tauri tree is ~57s.

## Before a PR

Everything under **Commands** except the dev and bundle commands is what CI runs. The
feature-level release gate is [`SMOKE_TEST.md`](SMOKE_TEST.md).
