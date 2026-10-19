# Silo docs

Design, reference, and process material for the (now-built) app. Start with the root
[README](../README.md) and [CONTRIBUTING](../CONTRIBUTING.md); the files below are the
deeper background. Suggested reading order:

1. **[MVP.md](MVP.md)** — v0.1 scope (and what's deliberately excluded).
2. **[DESIGN.md](DESIGN.md)** — design language, stack picks, farm color system,
   terminology approach, and the tinkerer power-layer (incl. the XML settings-form
   generator).
3. **[ARCHITECTURE.md](ARCHITECTURE.md)** — Tauri/Rust design, the shipped module map
   (incl. `modsroot` for mods-folder selection and `relocate` for the library archive
   move), data model, conflict engine, the projection engine.
4. **[CROSS-PLATFORM.md](CROSS-PLATFORM.md)** — Windows/macOS/Linux game-file
   discovery, the mods-folder override, per-OS projection, and the library archive on
   another drive.
5. **[DECISIONS.md](DECISIONS.md)** — why we chose what we chose.

Reference (mined knowledge):

- **[reference/fs25-modding-notes.md](reference/fs25-modding-notes.md)** — how FS25
  loads mods, `modDesc.xml` fields, conflict signals, projection mechanics.

Per-feature notes:

- **[COLLECTIONS.md](COLLECTIONS.md)** (design) and
  **[COLLECTIONS_WIRING.md](COLLECTIONS_WIRING.md)** (Phase 1 plan) — shareable mod-set
  links (in the app: *Share* on a mod set, and *Mod sets → Open a shared set*). Phase 1 has
  shipped; both docs are kept as written.
- **[BASE_SAVES.md](BASE_SAVES.md)** — base-save generator scope (not built).
- **[COMMUNITY_SERVER.md](COMMUNITY_SERVER.md)** — parked design.

Building, testing, releasing:

- **[BUILD.md](BUILD.md)** — building from source.
- **[SHOWCASE.md](SHOWCASE.md)** — the demo profile and screenshot capture for the website.
- **[RC_TESTING.md](RC_TESTING.md)** — release-candidate protocol (read first), then the
  **[SMOKE_TEST.md](SMOKE_TEST.md)** feature checklist.
- **[RELEASE_VALIDATION.md](RELEASE_VALIDATION.md)** — per-release validation records.
- **[VERIFY_DOWNLOAD.md](VERIFY_DOWNLOAD.md)** — verifying a download's signature,
  checksum, and build provenance.

The legal pages (privacy policy, terms, disclaimer, takedown) are on the website,
[silo.hllmr.com/legal/](https://silo.hllmr.com/legal/privacy-policy.html), maintained in
[HLLMR/web](https://github.com/HLLMR/web). Per-feature design
specs and plans live with each feature's issue and pull request.
