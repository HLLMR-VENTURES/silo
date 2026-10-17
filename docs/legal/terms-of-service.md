> **DRAFT — NOT LEGAL ADVICE.** This document was AI-drafted as a template for **HLLMR Ventures LLC**
> and has **not** been reviewed by an attorney. Do not publish or rely on it until a qualified lawyer
> has reviewed it and every `[bracketed placeholder]` has been completed. See `README.md` for the punch-list.

# Terms of Service

_Effective date: 2026-08-01_

These Terms of Service ("Terms") govern your access to and use of the Silo desktop application (the "App"), the website at silo.hllmr.com (including the marketing pages, the catalog browser at /browse, and the help documentation at /help) (the "Website"), and the public read API at silo-api.hllmr.com (the "API"). The App, the Website, and the API are together referred to as the "Services."

The Services are provided by **HLLMR Ventures LLC** ("HLLMR," "we," "us," or "our"). By downloading, installing, accessing, or using any of the Services, you agree to be bound by these Terms. If you do not agree to these Terms, do not use the Services.

## 1. About Silo

Silo is a free, open-source desktop application that helps you manage mods for Farming Simulator 25 ("FS25"). The Services include:

- **The App** — a desktop application that organizes your FS25 mod library on your own computer.
- **The Website** — marketing pages, a public catalog browser at /browse, and help documentation at /help.
- **The API** — a public, read-only service that provides the mod catalog data used by the App and the /browse catalog.

Silo is not a game, is not a mod, and does not include any FS25 mod files.

## 2. The Software Is Free and Open Source

The App is free of charge. There is no paid tier, no subscription, no advertising, and no user account.

The source code for the App is licensed under the MIT License and is available at github.com/HLLMR/silo. **The MIT License, not these Terms, governs your rights to use, copy, modify, and distribute the App's software itself.** In the event of any conflict between these Terms and the MIT License with respect to the software source code, the MIT License controls as to that software. These Terms govern your use of the Services as we operate them — in particular the Website, the /browse catalog, and the API.

## 3. Data Practices

We designed the Services to collect as little as possible about you. This Section describes what each part of the Services does. It is provided as part of these Terms for transparency; any separate privacy notice we publish also applies.

### 3.1 The App

The App has no telemetry, no analytics, no advertising, and no user account. It stores data only locally on your computer — a local SQLite cache, plus your operating system's keychain for any GitHub access tokens you choose to connect.

The App communicates only with the following, and only as described:

- **The API (silo-api.hllmr.com)** — to read the mod catalog. For lookups, the App sends mod technical names and file hashes. It does not send personal data.
- **The GitHub API** — optional and user-initiated. If you choose to sign in using GitHub's device-flow login, the resulting token is used only for explicit actions you take, such as starring or watching a repository. The token is never attached to mod downloads.
- **Mod source content delivery networks** (ModHub, GitHub) — to fetch images and, when you choose to, to download a mod.

Mod integrity and provenance hashing is performed locally on your computer.

### 3.2 The Website

The Website's static pages (the landing pages and /help) set no cookies and run no analytics. The only client-side storage is a "theme" preference kept in your browser's localStorage. The /browse page fetches catalog data from the public API from within your browser.

### 3.3 The API

To prevent abuse, the API applies per-IP rate limiting and keeps standard server logs. As a result, it processes IP addresses and request metadata on a transient basis. The API is (or will be) fronted by a content delivery network (Cloudflare), which also processes traffic for security and caching. The API has no accounts and sets no tracking cookies.

## 4. The Catalog

The catalog aggregates publicly available mod metadata — such as names, images, descriptions, versions, and ratings — from ModHub and GitHub. This information is deduplicated into a single record per mod, and each record links back to its original source.

**Silo does not host, mirror, or redistribute mod files.** The catalog is an index of publicly available data with links back to the original sources. When you download a mod, the download comes from the original source, not from us.

Catalog content — including mod names, artwork, descriptions, and other metadata — belongs to its respective authors and platforms. We make no claim of ownership over it and provide it only as an index and reference.

## 5. Acceptable Use

You agree to use the Services only for lawful purposes and in accordance with these Terms. In particular, you agree not to:

- Use the Services in violation of any applicable law or regulation.
- Access, query, or scrape the Website, the /browse catalog, or the API in a manner intended to abuse, disrupt, or gain unfair advantage from the Services, or that circumvents or attempts to circumvent rate limiting or other protective measures.
- Send automated requests to the API or Website at a volume or rate that is intended to, or that does, overwhelm, degrade, or impair the Services or the infrastructure that supports them.
- Attempt to gain unauthorized access to any part of the Services, or to any systems or networks connected to them.
- Interfere with, disrupt, or attempt to compromise the integrity, security, or proper functioning of the Services.
- Use the Services to infringe the intellectual property or other rights of HLLMR or any third party, or to distribute unlawful, harmful, or malicious content.

We may rate-limit, block, suspend, or restrict access to the Website or API to protect the Services and other users, including as described in Section 3.3.

## 6. Your Game Files and the Mods You Install

The App modifies files in your own FS25 game folder in order to manage your mods. These changes are designed to be reversible. **You use the App to modify your own game files at your own risk.** You are responsible for maintaining your own backups.

You are solely responsible for the mods you choose to install and use. Mods are created and distributed by third parties, and we do not create, host, review, endorse, or guarantee them.

**Provenance and any "Verified" status the App displays is an integrity and origin signal — not a security guarantee and not a virus scan.** It confirms whether a given build matches the source it claims to come from. It does not vouch for the safety, quality, or behavior of a mod, and it is not a substitute for your own judgment or your own security tools.

## 7. Third-Party Sources and Services

The Services link to and interoperate with third-party platforms, including ModHub and GitHub, and with mod content delivered from their content delivery networks. Your use of those platforms and any content obtained from them is governed by their own terms of service and privacy policies, not by these Terms. We are not responsible for third-party platforms, their content, their availability, or their practices.

If you connect a GitHub account to the App, you are responsible for complying with that platform's terms and for the security of your own credentials.

## 8. Intellectual Property and Trademarks

Silo is not affiliated with, endorsed by, or sponsored by GIANTS Software or the Farming Simulator brand. "Farming Simulator," "FS25," "ModHub," "GitHub," and all mod names and artwork are the property of their respective owners. All references to these names are for identification and descriptive purposes only.

The App's software source code is made available under the MIT License, as described in Section 2. Except as expressly stated in that license, these Terms do not grant you any right, title, or interest in HLLMR's name, logos, or branding.

## 9. Disclaimer of Warranties

THE SERVICES ARE PROVIDED "AS IS" AND "AS AVAILABLE," WITHOUT WARRANTY OF ANY KIND, TO THE FULLEST EXTENT PERMITTED BY APPLICABLE LAW. TO THAT EXTENT, HLLMR DISCLAIMS ALL WARRANTIES, WHETHER EXPRESS, IMPLIED, OR STATUTORY, INCLUDING BUT NOT LIMITED TO ANY IMPLIED WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE, TITLE, AND NON-INFRINGEMENT.

We do not warrant that the Services will be uninterrupted, secure, error-free, or free of harmful components, that the catalog data will be accurate, complete, or current, or that any defects will be corrected. As stated in Section 6, the App modifies files in your own game folder — designed to be reversible — at your own risk, and integrity or "Verified" status is not a security guarantee.

Some jurisdictions do not allow the exclusion of certain warranties, so some of the above exclusions may not apply to you.

## 10. Limitation of Liability

TO THE FULLEST EXTENT PERMITTED BY APPLICABLE LAW, HLLMR AND ITS MEMBERS, MANAGERS, OFFICERS, EMPLOYEES, AND CONTRIBUTORS WILL NOT BE LIABLE FOR ANY INDIRECT, INCIDENTAL, SPECIAL, CONSEQUENTIAL, EXEMPLARY, OR PUNITIVE DAMAGES, OR FOR ANY LOSS OF DATA, LOSS OF GAME SAVES, LOSS OF PROFITS, OR BUSINESS INTERRUPTION, ARISING OUT OF OR RELATING TO YOUR USE OF OR INABILITY TO USE THE SERVICES, WHETHER BASED ON WARRANTY, CONTRACT, TORT (INCLUDING NEGLIGENCE), OR ANY OTHER LEGAL THEORY, EVEN IF WE HAVE BEEN ADVISED OF THE POSSIBILITY OF SUCH DAMAGES.

BECAUSE THE APP IS PROVIDED FREE OF CHARGE, TO THE FULLEST EXTENT PERMITTED BY APPLICABLE LAW OUR TOTAL AGGREGATE LIABILITY ARISING OUT OF OR RELATING TO THE SERVICES OR THESE TERMS WILL NOT EXCEED ONE HUNDRED U.S. DOLLARS (US $100.00).

Some jurisdictions do not allow the exclusion or limitation of certain damages, so some of the above limitations may not apply to you.

## 11. Indemnification

To the fullest extent permitted by applicable law, you agree to indemnify, defend, and hold harmless HLLMR and its members, managers, officers, employees, and contributors from and against any claims, liabilities, damages, losses, and expenses (including reasonable legal fees) arising out of or related to: (a) your use of the Services; (b) your violation of these Terms; (c) your violation of any applicable law or the rights of any third party, including any third-party platform's terms; or (d) the mods you choose to install, use, or distribute.

## 12. Changes to These Terms

We may update these Terms from time to time. When we do, we will revise the "Effective date" above and post the updated Terms. Changes are effective when posted. Your continued use of the Services after the updated Terms take effect constitutes your acceptance of them. If you do not agree to the updated Terms, you must stop using the Services.

## 13. Changes to or Discontinuation of the Services

The Website and API are hosted services that we may modify, suspend, rate-limit, or discontinue at any time, in whole or in part, with or without notice. Because the App is open source under the MIT License, you may continue to use a copy you have already obtained subject to that license, even if we change or discontinue the hosted Services on which some App features depend.

## 14. Governing Law

These Terms are governed by and construed in accordance with the laws of the **State of Texas**, without regard to its conflict-of-laws principles, except to the extent that mandatory local law applies to you.

## 15. Severability and Entire Agreement

If any provision of these Terms is held to be unenforceable or invalid, that provision will be limited or eliminated to the minimum extent necessary, and the remaining provisions will remain in full force and effect. These Terms, together with the MIT License as it applies to the App's software, constitute the entire agreement between you and HLLMR regarding the Services and supersede any prior agreements on that subject. Our failure to enforce any provision is not a waiver of it.

## 16. Contact

Questions or notices regarding these Terms may be sent to **legal@hllmr.com**.

---

_This document is a draft and has not yet been reviewed by an attorney._

---
[← Legal index](./README.md)
