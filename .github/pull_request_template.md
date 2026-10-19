<!--
Thanks for contributing to Silo! Keep PRs focused and small where you can.
See CONTRIBUTING.md for the full flow and working principles.
-->

## What & why

<!-- What does this change do, and why? Link any related issue (e.g. Closes #123). -->

## Checklist

- [ ] **What/why** is described above (and an issue is linked if there is one).
- [ ] **Checks pass:** `npm test`, `npm run build` (svelte-check + build), and in `src-tauri/`:
      `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- [ ] **Tests** added or updated for changed core logic (modules unit-test without a running app).
- [ ] **Off-thread & reversible** where relevant — heavy work stays in Rust off the UI
      thread; any file write into the user's game files has an undo path.
- [ ] **XML** parsed with `quick-xml`, not regex (if this touches modDesc/savegame parsing).
- [ ] **Docs updated** (`README.md`, `docs/`, `CLAUDE.md`) if behavior or setup changed.

## Notes for reviewers

<!-- Anything worth calling out: tradeoffs, follow-ups, what you tested against. -->
