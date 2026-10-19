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
> **legal@hllmr.com**; server-log retention = **30 days**; DMCA designated agent registered
> with the U.S. Copyright Office (**DMCA-1083317**, active since 2026-10-17), with the
> agent's name, mailing address, phone and email listed on the takedown page.
>
> **Still needs a human:** attorney review of all four documents.

## Documents

| Document | In one line | Draft |
|---|---|---|
| **Privacy Policy** | The Silo Privacy Policy: no telemetry, analytics, ads or accounts; one table of every connection the app and website make; 30-day server access logs and the aggregate counts published from them; GDPR basics (controller, lawful basis, rights). | [privacy-policy.md](./privacy-policy.md) |
| **Terms of Service** | Terms of Service for Silo, HLLMR Ventures LLC's free, MIT-licensed FS25 mod manager, covering acceptable use of the website, /browse catalog, and public API; the catalog as a link-back index rather than a file host; as-is warranty disclaimer, liability limits, and indemnity; user responsibility for their own game files and installed mods; and third-party source terms. | [terms-of-service.md](./terms-of-service.md) |
| **Disclaimer & Trademarks** | A focused disclaimer for Silo covering no affiliation with GIANTS Software/Farming Simulator, trademark and catalog-content attributions, the "AS IS" and reversible-but-at-your-own-risk framing for file changes, and the key point that provenance/"Verified" is an integrity and origin signal, not a safety guarantee or antivirus scan. | [disclaimer.md](./disclaimer.md) |
| **Copyright & Takedown** | A DMCA-style notice-and-takedown policy for Silo's aggregated mod catalog, which indexes publicly available metadata and links back to the original source without hosting any mod files. | [copyright-takedown.md](./copyright-takedown.md) |

## Open items before attorney review

- **Attorney review:** all four documents are still AI-drafted and unreviewed. Keep the
  "DRAFT — NOT REVIEWED" banners until a lawyer has signed off.
- **Network and data disclosure:** the privacy policy now has one table of every connection
  (update check at launch, catalog requests only on user action, images, GitHub with and
  without sign-in, shared mod sets, downloads, website access logs and the counts on the Trust
  page). Keep it in sync with the code whenever a new connection is added; `SECURITY.md`
  points to it.
- **Hosting provider:** the website and API run on a rented virtual server. Confirm whether
  the provider and server region should be named in the privacy policy (as a processor and
  for international transfers).
- **ModHub content:** decided (owner, 2026-10-17): keep the existing ModHub image and hash
  cache, openly, to demonstrate the full platform should GIANTS allow a feed. New fetches from
  GIANTS' CDN stay paused. The Trust page says so; purge on GIANTS' request. Revisit once GIANTS
  replies.
- **Publisher identity:** settled. HLLMR Ventures LLC (Texas) owns Silo; the copyright
  line, these documents and the site use that name. Windows code signing stays under the
  founder's personal certificate (David Hellmer) by choice, and `docs/VERIFY_DOWNLOAD.md`
  explains that.
- **Effective date:** set one date across all four documents when they're finalized, then
  remove the "DRAFT — NOT REVIEWED" banner from `landing/legal/*.html`.
