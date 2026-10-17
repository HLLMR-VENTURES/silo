> **DRAFT — NOT LEGAL ADVICE.** This document was AI-drafted as a template for **HLLMR Ventures LLC**
> and has **not** been reviewed by an attorney. Do not publish or rely on it until a qualified lawyer
> has reviewed it and every `[bracketed placeholder]` has been completed. See `README.md` for the punch-list.

# Privacy Policy

_Effective date: 2026-08-01_

This Privacy Policy explains how **HLLMR Ventures LLC** ("HLLMR", "we", "us", or "our") handles information in connection with **Silo**, a free, open-source desktop application that manages Farming Simulator 25 (FS25) mods, together with the Silo website at silo.hllmr.com and the public read API at silo-api.hllmr.com (collectively, the "Services").

The short version: Silo is built to be **local-first**. The desktop app has no telemetry, no analytics, no ads, and no user account. It keeps your data on your own machine. The rest of this document explains the details honestly, including the limited, necessary processing that happens on our public API and through third-party services you choose to use.

## Who this policy covers

This policy applies to:

- The **Silo desktop app** (Windows, macOS, and Linux builds), whose source is MIT-licensed at github.com/HLLMR/silo.
- The **Silo website** at silo.hllmr.com, including the marketing landing page, the public catalog browser at /browse, and the help docs at /help.
- The **Silo public read API** at silo-api.hllmr.com.

It does not cover third-party services that Silo links to or interacts with, such as ModHub, GitHub, or Cloudflare. Those services have their own privacy policies, which are linked in the "Third-party services" section below.

## The desktop app is local-first

The Silo desktop app is designed so that your information stays on your own computer. Specifically:

- **No telemetry.** The app does not send usage data, event tracking, or crash analytics to us.
- **No analytics.** There is no product analytics or behavioral tracking of any kind.
- **No ads.** The app contains no advertising and no advertising trackers.
- **No account.** There is nothing to sign up for. We do not create, hold, or manage any user account for you.

### What the app stores, and where

The app stores data **only locally on your machine**:

- **A local SQLite cache**, used to track your mod library, loadouts, scan results, conflict and integrity findings, and similar working data. This lives on your device.
- **The operating system keychain**, used only to hold any GitHub access tokens that **you** choose to connect (see below). Tokens are stored in your OS keychain, not sent to us.

We do not receive, collect, or have access to this local data. If you uninstall the app or delete this data, it is gone from your machine; there is no copy on our side.

### The app modifies files in your own game folder

Silo manages your FS25 mods by organizing your mod files locally and projecting your active set into the game's `mods/` folder. These changes are made to files **in your own game folder on your own machine** and are designed to be reversible. This is a description of what the software does to your files locally; it is not a transfer of your data to us.

## Network connections the app makes

Although the app is local-first, it does connect to the network for specific, purpose-limited tasks. Here is every connection it makes and why:

- **The Silo public read API (silo-api.hllmr.com).** The app reads the mod catalog from our API for search, update checks, cover images, and integrity/provenance checks. To perform lookups, the app may send **mod technical-names and mod file hashes** so the API can identify a mod and return its catalog record. **No personal data is sent for these lookups.** Mod integrity and provenance hashing is performed **locally** on your machine.
- **The GitHub API (optional, user-initiated).** If you choose to log in to GitHub, the app uses GitHub's OAuth device flow. This login is used **only** for explicit actions you take, such as starring or watching a repository. The GitHub token is stored in your OS keychain and is **never attached to mod downloads**.
- **Mod source CDNs (ModHub, GitHub).** The app fetches images from these sources and, **when you choose**, downloads a mod from its original source. Silo does not host, mirror, or redistribute mod files; downloads always go to the original source.

Connections to GitHub and mod source CDNs are governed by those services' own privacy policies (linked below). When your app requests content from them, they will necessarily receive your IP address and standard request information as part of delivering that content.

## The website

The Silo website is intentionally minimal:

- **The static pages** (the landing page and the /help docs) set **no cookies** and run **no analytics**.
- **Client-side storage** on the site is limited to a single **"theme" preference stored in your browser's localStorage**, so the site can remember whether you prefer light or dark mode. This preference stays in your browser.
- **The /browse catalog** fetches mod data directly from the public read API (silo-api.hllmr.com) from your browser. When your browser makes that request, the API and its CDN receive your IP address and request metadata as described in the next section.

## The public read API (silo-api.hllmr.com)

The public read API serves the mod catalog. To keep the API available and to prevent abuse, it processes some information transiently:

- **IP addresses and request metadata**, used to apply **per-IP rate limiting** and standard server logging. This processing is transient and operational; the API has **no user accounts and sets no tracking cookies**.
- **A content delivery network (Cloudflare).** The API is (or will be) fronted by Cloudflare, which processes traffic for security and caching. Cloudflare therefore also processes IP addresses and request metadata on our behalf. See Cloudflare's privacy policy, linked below.

The catalog itself aggregates **publicly available mod metadata** (such as names, images, descriptions, versions, and ratings) from ModHub and GitHub, deduplicated into one record per mod, with links back to each original source. The catalog does not contain personal data about you.

## Third-party services

Silo interacts with the following third parties. Each has its own privacy policy that governs how it handles your information when you use it:

- **GitHub** (optional login and downloads) — https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement
- **ModHub / GIANTS Software** (mod source and images) — https://www.giants-software.com/privacy.php
- **Cloudflare** (CDN in front of the API) — https://www.cloudflare.com/privacypolicy/

We are not responsible for the privacy practices of these third parties. We encourage you to review their policies.

## Data retention

- **App data** (the local SQLite cache and any tokens in your OS keychain) lives on your machine and is retained until you delete it or uninstall the app. We do not hold it.
- **Server logs** for the public API (including IP addresses and request metadata) are retained transiently for rate-limiting, security, and operational purposes for **30 days**, after which they are deleted or aggregated. Cloudflare retains logs according to its own policies.
- **Website theme preference** remains in your browser's localStorage until you clear it.

## No tracking, no ads, no profiling

We do not build advertising profiles, we do not sell or rent personal information, and we do not track you across sites or apps. The Services contain no advertising trackers and no cross-site tracking cookies.

## Children's privacy

The Services are general-purpose software for managing game mods and are not directed to children. We do not knowingly collect personal information from children. Because the app is local-first and the API holds no accounts, we do not knowingly hold personal information about any child. If you believe a child has provided us with personal information through the Services, please contact us at **legal@hllmr.com** and we will address it.

## Your privacy rights

Depending on where you live, you may have rights over your personal information, such as the right to access, correct, delete, or restrict its processing, or to object to processing, under laws like the EU/UK General Data Protection Regulation (GDPR) or the California Consumer Privacy Act (CCPA).

In practice, the Services hold very little that identifies you: the app keeps your data locally on your own machine (which you can access or delete yourself at any time), and our API only processes IP addresses and request metadata transiently for rate-limiting and security. We do not sell personal information, and we do not use it for targeted advertising.

If you wish to make a data-subject request or have questions about your rights, contact us at **legal@hllmr.com**. We will respond as required by applicable law. Because we do not maintain accounts, we may be unable to identify or locate transient server-log data associated with a specific individual.

## International users

The Services are operated from **Texas, United States**, and our service providers (including Cloudflare) may process data in multiple locations. If you access the Services from outside that jurisdiction, you understand that limited operational data, such as IP addresses and request metadata processed by the API and its CDN, may be processed in countries other than your own, which may have different data-protection laws. We rely on the minimal, purpose-limited nature of this processing to protect your information.

## Security

We take reasonable measures to protect the limited information the Services process. The desktop app keeps your data on your device and stores connected tokens in your operating system's keychain. No method of storage or transmission is completely secure, and we cannot guarantee absolute security.

## No warranty

The software is provided "AS IS", without warranty of any kind, to the extent permitted by law. The app modifies files in your own game folder (and is designed to do so reversibly) **at your own risk**.

## A note on "Verified" / provenance

Silo's integrity and provenance feature is an **origin and integrity signal, not a security guarantee or virus scan**. It confirms whether an installed build matches the source it came from; it does **not** vouch for a mod's safety or scan it for malware. This is described here because people sometimes assume "Verified" means "safe" — it does not.

## Trademarks and affiliation

Silo is **not affiliated with, endorsed by, or sponsored by GIANTS Software or the Farming Simulator brand**. "Farming Simulator", "FS25", "ModHub", "GitHub", and individual mod names and artwork are the property of their respective owners. Mod content in the catalog belongs to its respective authors and platforms.

## Changes to this policy

We may update this Privacy Policy from time to time. When we do, we will revise the effective date at the top. Material changes will be reflected in the version published with the Services.

## Governing law

This Privacy Policy is governed by the laws of the **State of Texas**, without regard to its conflict-of-laws rules, except where applicable data-protection law provides otherwise.

## Contact us

If you have questions about this Privacy Policy or our data practices, contact:

**HLLMR Ventures LLC**
Email: **legal@hllmr.com**

---
[← Legal index](./README.md)
