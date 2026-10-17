> **DRAFT — NOT LEGAL ADVICE.** This document was AI-drafted as a template for **HLLMR Ventures LLC**
> and has **not** been reviewed by an attorney. Do not publish or rely on it until a qualified lawyer
> has reviewed it and every `[bracketed placeholder]` has been completed. See `README.md` for the punch-list.

# Privacy Policy

_Effective date: 2026-10-17_

This Privacy Policy explains how **HLLMR Ventures LLC** ("we", "us", or "our") handles information in connection with **Silo**, a free, open-source desktop application that manages Farming Simulator 25 (FS25) mods, together with the Silo website at silo.hllmr.com and the public read API at silo-api.hllmr.com (together, the "Services").

The short version: the Silo app has no telemetry, no analytics, no ads and no user account. Your library, saves, notes and settings stay on your computer. The app goes online only for the specific jobs listed in the table below, and our servers keep standard access logs for 30 days.

## Who is responsible

The controller for the Services is **HLLMR Ventures LLC**, a Texas limited liability company, USA. Contact: **legal@hllmr.com**.

This policy covers:

- The **Silo desktop app** (Windows, macOS and Linux builds). Its source is MIT-licensed at github.com/HLLMR/silo.
- The **Silo website** at silo.hllmr.com, including the landing page, /browse, /help, /trust and the shared-collection page /c/.
- The **Silo public read API** at silo-api.hllmr.com.

It does not cover GitHub, ModHub or other services Silo links to or talks to. They have their own privacy policies, linked under "Third-party services" below.

## What the app keeps on your computer

- **A local database** (`silo.db`) with your mod library index, loadouts, curation (favorites, ratings, tags, notes), source pages you saved, scan results and settings. It lives in the app's data folder on your device.
- **Your GitHub token**, if you connect GitHub. It goes into your operating system's credential store (Windows Credential Manager, macOS Keychain, Linux Secret Service). On a Linux system without a keyring, Silo stores it in its local database instead.

We do not receive this data. If you uninstall the app and delete its data folder, it is gone; there is no copy on our side.

Silo also moves and links mod files inside your own game folder, and writes backups there (`backups/`, `.bak` files, `SiloBackups`). That happens on your computer and sends nothing to us.

## Every connection Silo makes

The app has no background telemetry. Apart from the update check at launch, it connects only when you do something that needs it. Most requests from the app carry Silo's version in their User-Agent header (for example `Silo/1.2.3`), and every server the app reaches sees your IP address.

| What triggers it | Where it goes | What is sent |
|---|---|---|
| **App update check**: every time Silo starts | GitHub (`github.com/HLLMR/silo/releases/latest/download/latest.json`) | A plain request for that file. No library data. If you choose to install an update, the installer downloads from GitHub Releases. |
| **Browse**: opening the Browse tab, searching, filtering, sorting or paging | Our catalog API (silo-api.hllmr.com) | Your search terms, filters, sort order and page. |
| **A mod's details**: opening a mod's drawer in Library or Browse | Our catalog API | That mod's tech name (or its catalog id), to show its catalog record and whether a newer version exists. |
| **Check for updates** (⟳ Updates, or the Needs update filter) | Our catalog API | The tech names of your installed mods. Versions are compared on your computer. |
| **Verify integrity** | Our catalog API | The mod's tech name and version, to fetch the published build's file list. Hashing and the comparison happen on your computer. No file hashes are sent. |
| **Install from Browse** | Our catalog API, then GitHub | The catalog id and version, to find the download address and the published hash. The file itself downloads from the GitHub release. |
| **Collections**: publishing, previewing or importing | Our catalog API | The tech names of the mods in the collection, to find where each one can be installed from. |
| **Cover images** in Browse and the details drawer | Our catalog API's image cache; GitHub for GitHub-hosted images | The image address. Images on our API load directly in the app window; GitHub-hosted images are fetched by the app. |
| **GitHub, without signing in**: checking a mod linked to a GitHub repository for updates, showing repository stats in a drawer, installing a GitHub mod, opening a shared collection link | GitHub API (api.github.com) and GitHub release downloads | The repository name or gist id. If you have connected GitHub, your token is sent with these API requests so you get the higher rate limit. It is never sent with mod downloads. |
| **Connect GitHub** (sign-in) | GitHub (github.com/login/device, api.github.com/user) | GitHub's OAuth device flow. Silo asks for `read:user` (your username, for the higher rate limit). **Enable actions** adds `public_repo` (star, watch and public collections). **Enable collection sharing** adds `gist` (private collections). Silo never asks for access to private repositories. |
| **Star / Watch** | GitHub API | The repository name and your token. The star or watch lands on your GitHub account. |
| **Publishing a collection** | GitHub API | Creates a secret gist (private collection) or a public repository (public collection) in **your** GitHub account. It contains the collection name, your GitHub username, an optional description and the mod list (tech names, versions, sources and a hash of each mod's build). Silo also lists your gists and repositories to show *Your collections*. A secret gist is unlisted but anyone with its link can open it. |
| **Mod downloads and updates** | GitHub release assets | A plain download. No token. |
| **Links you open** (Open page ↗, ModHub pages, source pages you saved) | Your web browser | Silo hands the address to your browser. It never visits those pages itself. |

Silo never uploads your saves, mod archives, notes, settings or personal files.

## The website

- The website sets **no cookies** and runs **no analytics scripts**. The only thing it stores in your browser is your light/dark theme choice (in `localStorage`).
- **/browse** loads catalog data from our API in your browser. Cover images load from our API's image cache, or for some mods directly from where the source hosts them (GitHub or ModHub).
- **/trust** loads catalog counts and traffic totals from our servers and release statistics from the GitHub API, in your browser.
- **/c/** (shared collections) reads the collection from the GitHub API in your browser.

When your browser loads data from GitHub, GitHub sees your IP address under its own privacy policy.

## Our servers and their logs

The website and the API run on our own server (a rented virtual server we administer ourselves). No CDN or third-party analytics service sits in front of it.

- **Access logs.** Like most web servers, ours logs each request: IP address, time, the page or API path requested, and the user agent (your browser, or Silo's version). We use these logs to keep the Services secure and available, to rate-limit abuse, and to fix problems.
- **Retention.** Access logs are deleted after **30 days**.
- **Published counts.** From those logs we compute aggregate counts, such as requests per hour and unique visitors or app clients per month, and publish only the totals on the [Trust page](https://silo.hllmr.com/trust/). The totals contain no IP addresses and can't be traced back to you.
- **The catalog itself** contains public mod information (names, images, descriptions, versions, ratings and file hashes) gathered from ModHub and GitHub, with links back to each source. It contains no personal data about you.

## Why we process data (lawful basis)

If data-protection law such as the EU or UK GDPR applies to you:

- **Server access logs and published counts:** our legitimate interest in running secure, reliable Services and understanding how much they're used. We keep the logs short-lived and publish only totals.
- **Catalog requests from the app or website:** to provide the feature you asked for.
- **GitHub sign-in features** (star, watch, publishing collections): these run between your computer and GitHub. We don't receive your token or your GitHub data. You start each one and can stop at any time by disconnecting GitHub in Silo or revoking the app in your GitHub settings.

## What we don't do

- We don't sell or rent personal information.
- We don't show ads or use advertising trackers.
- We don't set tracking cookies or track you across sites or apps.
- We don't build profiles of users.

## Third-party services

These services have their own privacy policies:

- **GitHub** (app updates, sign-in, downloads, collections) — https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement
- **GIANTS Software / ModHub** (pages you open in your browser, and some cover images on /browse) — https://www.giants-software.com/privacy.php

We are not responsible for their practices.

## Your rights

Depending on where you live, you may have the right to **access** the personal data we hold about you, to have it **corrected** or **deleted**, to **object** to or restrict our processing, and to **complain to a data-protection supervisory authority** in your country.

In practice, the only personal data we hold is the IP address and user agent in our access logs, for up to 30 days. To make a request, email **legal@hllmr.com**. Tell us the IP address and the approximate time of the requests, because we have no accounts to look you up by. We will reply within one month.

## Children

The Services are not directed to children under 13, and we do not knowingly collect personal information from them. If you believe a child has sent us personal information, contact **legal@hllmr.com** and we will delete it.

## International users

We operate the Services from Texas, United States. If you use them from another country, your requests are processed outside your country, under different data-protection laws. We keep that processing to the access logs described above.

## Security

We take reasonable measures to protect what the Services process. The app keeps your data on your device and keeps a connected GitHub token in your operating system's credential store where one is available. No method of storage or transmission is completely secure.

## A note on "Verified"

Silo's integrity check confirms whether an installed build matches the build its source published. It is a provenance signal, not a virus scan, and it does not vouch for a mod's safety.

## Trademarks and affiliation

Silo is **not affiliated with, endorsed by, or sponsored by GIANTS Software or the Farming Simulator brand**. "Farming Simulator", "FS25", "ModHub", "GitHub" and individual mod names and artwork belong to their respective owners.

## Changes to this policy

We may update this policy. When we do, we change the effective date at the top, and we describe material changes in the release notes or on this page.

## Governing law

This policy is governed by the laws of the **State of Texas**, without regard to its conflict-of-laws rules, except where applicable data-protection law provides otherwise.

## Contact

**HLLMR Ventures LLC**, Texas, USA
Email: **legal@hllmr.com**

---
[← Legal index](./README.md)
