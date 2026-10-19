<script lang="ts">
  // Settings → Accounts: the GitHub connection. Its card has id "settings-accounts", the focus
  // target for go("settings", "accounts") (Browse's "connect GitHub" link).
  //   <AccountSettings />
  import { onMount } from "svelte";
  import Icon from "../../ui/Icon.svelte";
  import { secretStorageSecure } from "../../api";
  import GitHubAuth from "../GitHubAuth.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import { ACCOUNTS_SECTION_ID } from "./sections";

  // When the OS keychain is unavailable, connecting an account falls back to storing the
  // token in the local DB unencrypted — warn before the user connects rather than silently.
  let keychainOk = $state(true);
  onMount(async () => {
    try {
      keychainOk = await secretStorageSecure();
    } catch {
      keychainOk = true; // don't cry wolf if the probe itself errors
    }
  });
</script>

<SettingsCard id={ACCOUNTS_SECTION_ID} title="GitHub account">
  {#if !keychainOk}
    <div class="keychain-warn">
      <Icon name="problem" size={14} /> This computer has no secure password store Silo can use, so your
      GitHub sign-in would be saved <strong>unencrypted</strong> on this computer. Connect only if
      you're comfortable with that. Disconnecting removes it.
    </div>
  {/if}
  <GitHubAuth />
</SettingsCard>

<style>
  .keychain-warn {
    font-size: var(--fs-sm);
    line-height: 1.5;
    color: var(--text);
    background: var(--danger-soft);
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--border));
    border-radius: var(--r-sm);
    padding: var(--sp-2) var(--sp-3);
  }
</style>
