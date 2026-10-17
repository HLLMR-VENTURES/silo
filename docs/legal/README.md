# Silo — legal documents (DRAFTS)

Draft legal terms for the Silo public site, owned/operated by **HLLMR Ventures LLC**.
These `.md` files are the editable source; they are rendered to styled pages at
`landing/legal/*.html` and are **live at silo.hllmr.com/legal/ with a prominent "DRAFT — NOT
REVIEWED" banner** on each page (published in good faith by the owner's decision, to signal
good-faith compliance while review is pending).

> ## ⚠️ These are AI-drafted templates, not legal advice — still pending attorney review
> Published as clearly-marked drafts, but **not yet reviewed by a lawyer**. Get them reviewed and
> update both the `.md` source and the rendered `landing/legal/*.html` when finalized.
>
> **Filled in:** state of formation + governing law = **Texas, United States**; contact =
> **legal@hllmr.com**; server-log retention = **30 days**; DMCA notices accepted by email.
>
> **Still needs a human:** a physical mailing address for the DMCA **Designated Agent** and a
> formal **U.S. Copyright Office** agent registration (~$6) to actually secure DMCA safe-harbor
> — the takedown page currently operates by email only and says so.

## Documents

| Document | In one line | Draft |
|---|---|---|
| **Privacy Policy** | The Silo Privacy Policy: the app is local-first with no telemetry, analytics, ads, or accounts; it stores data only on your machine and talks to the public catalog API (which processes IP/logs transiently for rate-limiting behind Cloudflare) and optional third-party services you initiate. | [privacy-policy.md](./privacy-policy.md) |
| **Terms of Service** | Terms of Service for Silo, HLLMR Ventures LLC's free, MIT-licensed FS25 mod manager, covering acceptable use of the website, /browse catalog, and public API; the catalog as a link-back index rather than a file host; as-is warranty disclaimer, liability limits, and indemnity; user responsibility for their own game files and installed mods; and third-party source terms. | [terms-of-service.md](./terms-of-service.md) |
| **Disclaimer & Trademarks** | A focused disclaimer for Silo covering no affiliation with GIANTS Software/Farming Simulator, trademark and catalog-content attributions, the "AS IS" and reversible-but-at-your-own-risk framing for file changes, and the key point that provenance/"Verified" is an integrity and origin signal, not a safety guarantee or antivirus scan. | [disclaimer.md](./disclaimer.md) |
| **Copyright & Takedown** | A DMCA-style notice-and-takedown policy for Silo's aggregated mod catalog, which indexes publicly available metadata and links back to the original source without hosting any mod files. | [copyright-takedown.md](./copyright-takedown.md) |

## Open items before attorney review

- **DMCA designated agent:** a physical mailing address and a U.S. Copyright Office agent
  registration (about $6). Until then the takedown page works by email only and says so.
- **Network and data disclosure:** the privacy policy must match what the app and SiloAPI
  actually send: the update check on launch (github.com), catalog lookups (the installed
  tech-name list plus IP), images, GitHub sign-in, gists and stars, and the access-log
  counts published on the Trust page. Use one table for the privacy policy, `SECURITY.md`
  and the Trust page.
- **GDPR basics:** lawful basis for server logs (legitimate interest), retention, Cloudflare
  as processor, and the right to complain to a supervisory authority.
- **ModHub content:** the catalog's handling of ModHub images and metadata is under
  discussion with GIANTS. Revisit the "doesn't host or mirror" wording once that's settled.
- **Publisher identity:** align the copyright line, these documents and the code-signing
  identity on one legal name.
- **Effective date:** set one date across all four documents when they're finalized, then
  remove the "DRAFT — NOT REVIEWED" banner from `landing/legal/*.html`.
