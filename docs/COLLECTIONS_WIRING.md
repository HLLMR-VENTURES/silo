# Collections — Phase 1 wiring plan

> **Status: shipped** (epic #61, PRs #63–#69). Kept as the original plan; some components
> ended up with other names. In Silo 1.0 the feature is *sharing a mod set* and *Mod sets →
> Open a shared set*.

> Companion to [`COLLECTIONS.md`](COLLECTIONS.md) (the parked design). This is the
> concrete, code-grounded plan for **Phase 1: share a mod-set list via a GitHub
> secret gist**. Written 2026-08-01 after a subsystem survey; **for review, not yet
> an issue/PR.** P2 (public repos) and P3 (SiloAPI discovery) are out of scope here
> but the format stays forward-compatible with both.

## TL;DR

Collections is **assembly, not invention.** A collection is a portable *list* of
mods (a superset of today's `.silo` loadout file) — never the ZIPs. Importing one
walks each entry through Silo's existing **resolve → download → verify → materialize**
pipeline. ~90% of the machinery already ships. The net-new surface is: a collection
file format, three GitHub gist functions, three Tauri commands, one preview
component, and **one new OAuth scope (`gist`)**.

## What already exists (reused as-is)

| Need | Reused piece | Location |
|---|---|---|
| Per-mod version+hash+size entry (already `Serialize`, camelCase) | `mpsync::ManifestEntry` | `mpsync.rs:31` |
| Build hashed set from active mods (parallel, off-thread) | `mpsync::hash_mods` / `build_manifest` | `mpsync.rs:90,95` |
| Host↔joiner diff (missing / version / hash / extra) | `mpsync::diff → VerifyReport` | `mpsync.rs:105` |
| Canonical cross-source provenance hash | `provenance::manifest_from_zip` / `compare` | `provenance.rs:122,203` |
| Resolve a source + canonical hash for a version | `siloapi::resolve_download` (+ `manifest`) | `siloapi.rs:314,284` |
| GitHub-install vs open-page split (API owns `installable`) | `install_remote_mod` body | `lib.rs:593` |
| Download + identity gate (PK magic, root modDesc, sha256 refusal) | `siloapi::download_to` | `siloapi.rs:406,487` |
| Verdict Verified/Modified/Unverified | `verify_mod` → `provenance::compare` | `lib.rs:695` |
| Persist + activate a loadout (reversible projection) | `db::save_loadout`, `set_active` | `db.rs:268`, `lib.rs:846` |
| Read→write OAuth scope escalation (the model to copy) | `gh_device_start/poll` `write:bool` seam | `lib.rs:236,259` |
| Secret token store + `api.github.com` allowlist | `secrets::get`, `is_github_host` | `secrets.rs`, `github.rs:205` |
| Dry-run "nothing has moved yet" modal | `OrganizePreview.svelte` | frontend |
| Per-row verified/modified/unverified badge + diff | `ProvenanceCheck.svelte` | frontend |

**Newly written:** a `collection.rs` format module, three `github.rs` gist
functions, three `#[tauri::command]`s, and `ImportPreview.svelte` +
`CollectionExport.svelte`. Everything security-load-bearing
(`validate_outbound_url`/SSRF, `safe_file_name`, `primary_root` trust boundary, the
`download_to` identity gate, `is_github_host`, the ratified manifest format) is
inherited unchanged — **the import flow adds no new trust boundary.** The only new
outbound writes are to `api.github.com` (gist create), already allowlisted.

## The collection format

A **new struct**, not a mutation of `LoadoutFile` — v1 `LoadoutFile.mods` is
`Vec<String>` while a collection's `mods` is `Vec<object>`, so they are not a
transparent serde superset. Keep them distinct files with distinct tags:

- `.silo` (`LoadoutFile`, `silo:1`) stays the local bare-techName loadout interchange (`lib.rs:905`).
- `silo-collection.json` (`schema:"silo.collection/1"`) is the richer shared artifact.

A collection **down-imports** to a loadout trivially (drop everything but techNames
→ `save_loadout`). `import_loadout` stays untouched; collection import is a separate command.

```rust
// new: src-tauri/src/collection.rs
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Collection {
    schema: String,                 // "silo.collection/1" — validated on read
    name: String,
    description: Option<String>,
    author: Option<String>,         // GitHub login, stamped at export
    created_at: Option<String>,     // RFC3339, client-stamped
    savegame: Option<SaveBinding>,  // optional MP/save pin
    mods: Vec<CollectionMod>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionMod {
    tech_name: String,
    version: Option<String>,
    source: Option<String>,         // "github" | "modhub" — preferred
    source_url: Option<String>,     // page/release URL for the open-page branch
    manifest_hash: Option<String>,  // canonical provenance hash, when known
    installable: Option<bool>,      // false → importer opens the page
    catalog_id: Option<String>,     // SiloAPI mod id, skips a lookup on resolve
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveBinding { name: String, map_title: Option<String> }
```

Every field beyond `tech_name` is optional and populated from the catalog
(`BrowseMod`/`ModSource`, `siloapi.rs`) + `provenance` at export time.

## New Rust — `github.rs` (pure, unit-testable; mirror `set_star`/`set_watch`)

```rust
/// POST /gists  (public:false = secret/unlisted). Needs the `gist` scope.
pub fn create_secret_gist(token: &str, name: &str, desc: &str, json: &str) -> Result<GistRef, String>;
//   body: { "description": name, "public": false,
//           "files": { "silo-collection.json": { "content": json } } }
//   → GistRef { id, html_url, raw_url }

/// GET /gists/{id} → files["silo-collection.json"].content.
/// Owner read needs `gist`; anonymous raw-URL read works for import-by-link.
pub fn read_gist(id: &str, token: Option<&str>) -> Result<String, String>;

/// Parse a pasted gist URL/id → gist id. Accept gist.github.com/{user}/{id},
/// gist.github.com/{id}, bare ids, api.github.com/gists/{id}.
pub fn parse_gist_ref(input: &str) -> Option<String>;
```

Both hit `api.github.com` (already covered by `is_github_host` — no new
credential-leak surface). Use the exact ureq shape of `set_star`/`set_watch`
(`github.rs:528`): `.set("Accept","application/vnd.github+json")
.set("User-Agent", UA).set("Authorization", &format!("Bearer {token}"))`, errors
through `gh_err`. Keep P2 helpers (`create_public_repo`, `put_repo_file`) as
TODO stubs only.

## New Rust — the `gist` scope escalation

Generalize the existing `write: Option<bool>` seam rather than inventing a new mechanism.

- `gh_device_start` / `gh_device_poll` (`lib.rs:236,259`): add a `gist: Option<bool>`
  param and build the scope additively —
  ```rust
  let mut scopes = vec!["read:user"];
  if write.unwrap_or(false) { scopes.push("public_repo"); }
  if gist.unwrap_or(false)  { scopes.push("gist"); }
  let scope = scopes.join(" ");
  ```
  GitHub OAuth apps **accumulate** scopes across authorizations, so a re-connect
  with the superset upgrades the token in place (identical to today's read→write).
- On poll success, alongside `gh_write`, set `gh_gist`.

**The one real design decision — scope tracking:**
- **Option A (minimal, P1-sized):** a `gh_gist` boolean flag mirroring `gh_write`.
  Fast, matches existing code exactly.
- **Option B (recommended fast-follow):** stop discarding the token response's
  `scope` field in `github::device_poll`, store a `gh_scopes` string, derive
  `canGist`/`canWrite` from it. Also fixes `gh_set_pat` setting `gh_write=1`
  without verifying the PAT carries the permission.
- **Recommendation:** ship A for P1 to stay small; file B as an issue. Until B, a
  PAT user can show write-capable yet 403 on gist create — lean on the `gh_err`
  403 mapping ("reconnect with collection sharing enabled") to recover.

## New Rust — three `#[tauri::command]`s

All `spawn_blocking`, `secrets::get(&conn,"gh_token")` for the token, registered in
`invoke_handler!` (~`lib.rs:1237`).

1. **`collection_export(app, loadout_id, savegame_folder: Option<String>, private: bool) -> String`** (share URL)
   - Load loadout → `mpsync::hash_mods` for `version`/`hash` → enrich each entry
     from the catalog (`siloapi::lookup`/`detail`) for
     `source`/`source_url`/`manifest_hash`/`installable`/`catalog_id` → stamp
     `author` (`gh_user`) + `created_at`.
   - Serialize `Collection` → `github::create_secret_gist` → return `html_url`.
   - `dir`/dev mods hash to `("",0)` today → warn-and-omit ("can't be shared — dev mod"),
     don't emit a silent zero-hash entry.

2. **`collection_import_preview(app, url_or_id: String) -> ImportPlan`**
   - `parse_gist_ref` → `read_gist` → parse + validate `schema` starts with `"silo.collection/"`.
   - **Resolve every entry first (cheap, network-only), then show the modal.** Per mod,
     `siloapi::resolve_download(base, catalog_id_or_lookup, want=source)`:
     - `Ok(ResolvedDownload)` → bucket **will-install**.
     - `Err("No source allows a direct download…")` → bucket **open-page** (carry `source_url`).
     - Cross-check local library (`catalog_check_updates`/`lookup` by techName):
       present-and-same-version → **already-present** (skip); present-but-different-version →
       **version-drift** row (reuse mpsync `version_mismatch` semantics), not silently satisfied.
   - Return `ImportPlan { will_install, open_page, already_present, version_drift, savegame }`. No file writes.

3. **`collection_apply(app, plan, on_progress) -> ApplyReport`**
   - Per will-install row, reuse `install_remote_mod`'s body verbatim:
     `safe_file_name` → `dest = root.join(filename)` → **skip if `dest.exists()`** →
     `siloapi::download_to(url, dest, expected_sha256, on_progress)` (streams to
     `.part`, PK magic + root `modDesc.xml` check, refuses on sha256 mismatch before rename).
   - After each install, `verify_mod(techName, version, path)` → `provenance::compare`;
     if the entry carries `manifest_hash`, compare `LocalManifest.manifest_hash` → per-row
     verdict Verified / Modified / Unverified.
   - Materialize: `db::save_loadout(conn, None, name, &techNames)`; if `savegame` present,
     persist `savegame_folder` on the loadout row (additive
     `ALTER TABLE loadout ADD COLUMN savegame_folder TEXT`, ignore-dup pattern `db.rs:138`).
     Optionally `set_active` to project it.
   - Open-page mods are saved into the loadout by techName, flagged "not installed —
     get from ModHub."
   - **MP path:** the joiner can additionally run `mpsync::diff` (manifest vs. local
     hashed set) for the four-bucket fix-list — `mp_verify_file` shape unchanged; the
     collection's per-mod `version`+`hash` *are* `ManifestEntry` fields.

## Frontend

- **`types.ts`**: add `canGist` (or `scopes`) to `GhStatus`; add `Collection`,
  `CollectionMod`, `SaveBinding`, `ImportPlan`, `PlanRow`, `ApplyReport`.
- **`api.ts`** (~line 243): `collectionExport`, `collectionImportPreview`,
  `collectionApply`; extend `ghDeviceStart`/`ghDevicePoll` to pass the `gist` intent.
- **`GitHubAuth.svelte`**: clone the `!status.canWrite` "Enable actions" row
  (~lines 138–145) into an **"Enable collection sharing"** row shown when the user
  tries to export and `canGist` is false → `connect({gist:true})`, device code
  rendered inside the connected branch (same `flowDisplay` snippet constraint).
- **New `ImportPreview.svelte`** — structural copy of `OrganizePreview.svelte`:
  same modal chrome, `onConfirm`/`onCancel`, `loading`/`applying`, "Nothing has been
  downloaded yet" copy, "Don't preview next time" flag. Group rows by bucket
  (Will install / Open page / Already in library / Update to pinned); post-apply,
  each will-install row shows a verdict badge.
- **Per-row verdict**: reuse `ProvenanceCheck.svelte`'s verdict/diff block and its
  `provenanceCache.svelte` memoization.
- **New `CollectionExport.svelte`** (or a Loadouts-panel action): pick loadout +
  optional savegame binding, Private toggle, "Create share link" → shows the URL
  with a copy button. **Explicit consent line:** "Generated by Silo, written to your
  GitHub account as a secret gist." A secret gist is *unlisted, not auth-gated* — say so.

## Phased PRs (each small, green CI, per the GH process standing order)

1. **Format + module** — `Collection`/`CollectionMod` in `collection.rs`,
   schema-validate on read, unit tests for round-trip + `parse_gist_ref`. No UI.
2. **Gist scope escalation** — `gist` param on `gh_device_start/poll`, `gh_gist`
   flag (Option A), `GhStatus.canGist`, `GitHubAuth.svelte` row, `gh_err` message
   tweak. File the Option-B scope-capture issue.
3. **Gist create/read** — `create_secret_gist`/`read_gist`/`parse_gist_ref` in
   `github.rs`, unit tests. `is_github_host` already covers it.
4. **Export** — `collection_export` command + `CollectionExport.svelte`
   (make-a-share-link from a loadout, dev-mod warning, consent copy).
5. **Import** — `collection_import_preview` + `collection_apply` +
   `ImportPreview.svelte` (resolve-all-first, dry-run modal, install+verify loop,
   loadout materialize, `savegame_folder` migration, per-row provenance badge).
6. **(Optional) MP diff hook** — surface `mpsync::diff` for a host↔joiner collection.

## Open questions / decisions to confirm

1. **Scope tracking: Option A (`gh_gist` flag) vs Option B (`gh_scopes` capture).**
   Recommend A for P1 speed, B as fast-follow (B also fixes the PAT gap).
2. **PAT users** with Option A show action-capable but may 403 on gist create.
   Acceptable for P1 if the 403 maps to a clear "reconnect / use a gist-scoped token"?
3. **Which hash in the entry.** Recommend `manifestHash` (canonical/provenance) as
   the primary trust field; `mpsync` MD5 stays implicit for the MP-diff path. Both, or just provenance?
4. **"Private" honesty copy** — confirm wording ("share this link only with your group").
5. **Idempotency vs version drift** — recommend present-but-different-version gets
   its own "update to pinned" row, not silently satisfied.
6. **Savegame binding persistence** — recommend a nullable `savegame_folder` column
   on `loadout` (additive migration) over a separate table.
7. **`dir`/dev mods in an export** — recommend warn-and-omit for a shareable artifact.
8. **Version drift on ModHub pins** — confirm the preview shows "pinned
   version no longer served → open page for latest" as correct behavior, not an error.

## Coverage / expectations

- Provenance verifies most rows by ModHub build (P3 live, ~6,300 versions,
  popular-first; **96% of real installs are ModHub-latest**). GitHub P1 (~128/141).
- `mpsync` loads whole zips into RAM for MD5 — fine now, a noted scaling edge for
  big-map collections (streaming is a deferred improvement).
