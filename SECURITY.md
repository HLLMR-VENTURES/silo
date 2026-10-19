# Security Policy

Silo is a native desktop app that manages your Farming Simulator 25 mod library.
We take its security seriously — it reads and writes files on your machine and
talks to a handful of network services, so we want to hear about anything that
could put users at risk.

## Reporting a vulnerability

**Please do not open a public issue for a security vulnerability.**

1. **Preferred:** use GitHub's
   [private vulnerability reporting](https://github.com/HLLMR/silo/security/advisories/new)
   on `HLLMR/silo` (Security → Report a vulnerability). This keeps the report
   private between you and the maintainers until a fix is out.
2. **If you can't use that**, open a minimal public issue on
   [HLLMR/silo](https://github.com/HLLMR/silo/issues) that says only that you've
   found a security problem and asks for a private channel — **without** the
   details, proof-of-concept, or anything that would let others exploit it. A
   maintainer will get you somewhere private to share the specifics.

Please include, when you can: the Silo version, your OS, what the issue is, and
the steps or a proof-of-concept to reproduce it. We'll acknowledge your report,
keep you updated on the fix, and credit you when it ships (unless you'd rather
stay anonymous).

## Scope

**In scope** — the Silo desktop application in this repository:

- The Rust core (`src-tauri/`) and the Svelte frontend (`src/`).
- Local file handling: library scanning, projection (symlink/junction/copy),
  savegame backups, and any write into
  `Documents/My Games/FarmingSimulator2025/`. Path traversal, unsafe extraction,
  or a non-reversible write that could damage a user's game files are all
  in-scope concerns.
- Silo's outbound network calls. Silo talks to:
  - **`silo-api.hllmr.com`** — the SiloAPI catalog backend, for cross-source mod
    metadata, only when the user browses, opens a mod, checks for updates, verifies a mod,
    installs from Browse, or shares or opens a mod set.
  - **GitHub** — the app update check at launch, release downloads, update checks for
    linked repos, shared mod set links, and (with the user's own OAuth token) star / watch and
    sharing mod sets as links (secret gists). The token stays on the user's machine; no server of ours holds
    or proxies it.
  - The full list is the table in the
    [privacy policy](docs/legal/privacy-policy.md#every-connection-silo-makes).

**Generally out of scope:**

- Vulnerabilities in FS25 itself, GIANTS software, GitHub or ModHub —
  report those to the respective vendor. (A flaw in how *Silo* interacts with
  them is in scope.)
- Issues that require a machine already compromised by a local attacker, or that
  depend on a malicious mod the user chose to install and run in the game.
- The SiloAPI server and the website live in separate, private repositories. Report
  issues with `silo-api.hllmr.com` or `silo.hllmr.com` here, the same way, and we'll
  route them.

## Privacy posture: no telemetry, no account

Silo has **no analytics, no telemetry, and no account system** — there is
nothing to sign up for and nothing phones home about your usage. Apart from the
app update check at launch (GitHub), its outbound traffic happens only when the
user asks for something: catalog lookups to `silo-api.hllmr.com`, GitHub
downloads and update checks, and actions taken through the user's own GitHub
account. This is intentional and part of the app's trust model; a
change that quietly adds tracking or exfiltrates data would itself be treated as
a security issue.

## Supported versions

| Version | Supported |
|---------|-----------|
| 1.0.x (latest release) | Yes |
| Older than 1.0 | No — update to the latest release |

Only the latest release gets security fixes; the in-app updater moves you to it. Fixes land
on `main` and ship in the next release. Please test against the latest release or a current
build from source before reporting.
