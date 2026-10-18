// Silo's public links, in one place (Help, Settings and the GitHub cards use them). Keep
// every outbound URL here.
//   onopen(DISCORD_URL)

/** The website (package.json `homepage`). */
export const WEBSITE_URL = "https://silo.hllmr.com";
/** The source code (package.json / Cargo.toml `repository`). */
export const SOURCE_URL = "https://github.com/HLLMR/silo";
export const DISCUSSIONS_URL = "https://github.com/HLLMR/silo/discussions";
export const DISCORD_URL = "https://discord.gg/NpxcYdGHrW";
export const NEW_ISSUE_URL = "https://github.com/HLLMR/silo/issues/new/choose";
/** What changed in each release (CHANGELOG.md at the repository root). */
export const CHANGELOG_URL = `${SOURCE_URL}/blob/main/CHANGELOG.md`;
/** Help → Report a problem: the bug report form (.github/ISSUE_TEMPLATE/bug_report.md). */
export const BUG_REPORT_URL = `${SOURCE_URL}/issues/new?template=bug_report.md`;
/** Every release, each with its installers and a software bill of materials (SBOM). */
export const RELEASES_URL = `${SOURCE_URL}/releases`;
/** Silo's own license (LICENSE at the repository root). */
export const LICENSE_URL = `${SOURCE_URL}/blob/main/LICENSE`;
/** GitHub: register an OAuth app (Settings → Accounts, the device-flow client id). */
export const GITHUB_NEW_OAUTH_APP_URL = "https://github.com/settings/applications/new";
/** GitHub: create a fine-grained personal access token. */
export const GITHUB_NEW_PAT_URL = "https://github.com/settings/personal-access-tokens/new";
/** A GitHub repository's page. */
export const githubRepoUrl = (owner: string, repo: string) => `https://github.com/${owner}/${repo}`;
