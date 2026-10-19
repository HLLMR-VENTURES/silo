# Silo Community Server — design (parked)

**Status: PARKED.** Do not start until *after* the first official public release. Sequence is
explicit: ship 1.0 (the mod-manager/diagnostic product) → *then* stand this up. This doc exists
so the plan is ready to execute, not to pull focus now.

## Thesis

Stand up **one** official **"Silo Community Server"** — a Farming Simulator 25 multiplayer server
— as a **dogfood + demo + community asset**, not a hosting business. Its whole reason to exist is
to make Silo's own features visible and to give the Discord community a place to play.

The load-bearing tie-in is **multiplayer mod-set sync** (`mpsync`): the server publishes its active
mod set as a Silo manifest, and members use Silo to verify/sync their mods against it. That feature
currently ships with no showcase; the server *is* the showcase.

## Why it's worth it (ecosystem value)

- **Dogfoods `mpsync`** — the export-manifest / verify / fix-list loop, exercised for real.
- **Demonstrates the value prop live** — a server running a **provenance-verified, conflict-checked**
  mod set is Silo's entire pitch, playable.
- **Anchors the community** — gives the Discord (and the bot) a reason to gather beyond support.
- **Content** — "join the Silo community server" is a clean hook for videos/streamers/posts.

## Non-goals (guardrails against scope creep)

- **Not** a paid hosting product or a GSP competitor.
- **Not** multi-server. Exactly one, curated, official server.
- **Not** self-hosted on the production VPS (see below).

## Build vs. rent — **rent a managed GSP**

Rent one managed FS25 server from a game-server provider (Nitrado, g-portal, or an FS-community
host), ~**$15–30/mo** depending on slots. Reasons **not** to self-host:

- The FS25 **dedicated server is a Windows program** and **consumes a game activation** (you must own
  FS25; the server uses a license). The VPS is Linux (Hetzner + Docker); running it via Wine in a
  container is unofficial and fiddly.
- A real-time physics/mod **sim is a bad co-tenant** for the production API + site + Discord bot —
  it would contend for CPU/RAM and put a game server's uptime in the same blast radius as the API.
- GSPs handle the Windows host, activation, admin web panel, mod upload, saves, and updates —
  removing the exact ops burden that would otherwise distract a small team.

Revisit self-hosting or hosting-as-a-feature only if real demand appears **post-1.0**.

## Silo integration (the point)

1. **Curate the set in Silo.** Build a loadout that is conflict-free (Silo's conflict detection) and
   provenance-**Verified** where possible. This is the server's canonical mod set.
2. **Export the manifest.** Use Silo's `mpsync` export to produce the hashed mod-set manifest for the
   server's active set.
3. **Publish it** where members can reach it — e.g. a pinned Discord post, a `silo.hllmr.com/server`
   page, or a raw manifest URL.
4. **Members sync.** In Silo: verify their set against the published manifest → get the fix-list
   (missing / wrong version / different file / extra) → resolve. Then they can join.
5. **(Future client feature, optional)** a "server profile" in Silo that takes a manifest URL and
   one-click-reconciles the local set to it. Not required for v1 — the existing verify flow works.

## Operations (keep it bounded)

- **Admin:** one owner via the GSP web panel. Moderation of players via the existing Discord roles.
- **Mod-set updates:** re-curate + re-export the manifest when the set changes (match Silo's own
  update cadence); announce the new manifest in Discord.
- **Saves:** GSP handles backups; keep a periodic off-box copy.
- **Uptime expectations:** set them honestly ("community server, best-effort") — do not promise SLAs.

## Launch checklist (when unparked)

- [ ] Pick a GSP + slot count; provision the server.
- [ ] Allocate/activate an FS25 license for the dedicated server.
- [ ] Curate the mod set in Silo (conflict-free, Verified-where-possible); load it on the server.
- [ ] Export the `mpsync` manifest; publish it (Discord pin + a link).
- [ ] Discord: announce + a "server" role/channel; the bot can post the manifest link.
- [ ] Content: a short "how to join with Silo" clip.
- [ ] Monitor players + `mpsync` usage; iterate the set.

## Costs & risks

- **Cost:** GSP monthly (~$15–30) + one FS25 activation. Low, predictable.
- **Risks:** ops creep, added support load, focus dilution. **Mitigations:** single server, fully
  GSP-managed, explicitly not a product, best-effort framing, capped scope.

## Trigger to unpark

After the **first official public release** is out and stable, *and* the Discord community shows
appetite for a shared place to play. Until both are true, this stays parked.
