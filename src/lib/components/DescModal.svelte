<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { openExternal } from "../api";
  import { tooltip } from "../ui/tooltip";

  let {
    modal,
    onClose,
  }: {
    modal: {
      title: string;
      text: string;
      url: string | null;
      loading: boolean;
      source: "catalog" | "summary";
    };
    onClose: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="modal-backdrop" onclick={onClose}></div>
<div class="modal" role="dialog" aria-modal="true">
  <div class="modal-head">
    <h3>{modal.title}</h3>
    <button class="drawer-x" use:tooltip={"Close"} aria-label="Close" onclick={onClose}><Icon name="close" /></button>
  </div>
  <div class="modal-body">
    {#if modal.text}
      <p>{modal.text}</p>
    {:else}
      <p class="modal-empty">No description text available.</p>
    {/if}
    {#if modal.loading}
      <p class="modal-loading">Loading the full description…</p>
    {/if}
  </div>
  <div class="modal-foot">
    <span class="modal-note">
      {#if modal.source === "catalog"}
        Full body from the catalog. The source page has the original formatting.
      {:else}
        Catalog summary — the full write-up lives on the mod's page.
      {/if}
    </span>
    {#if modal.url}
      <button class="modal-link" onclick={() => openExternal(modal.url!)}>
        Open full mod page <Icon name="external" size={13} />
      </button>
    {/if}
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 50;
  }
  .modal {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(620px, 92vw);
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    background: var(--surface, var(--bg));
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.35);
    z-index: 51;
  }
  .modal-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .modal-head h3 {
    margin: 0;
    font-size: 1rem;
  }
  .drawer-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 1rem;
    cursor: pointer;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .drawer-x:hover {
    background: var(--bg);
    color: var(--text);
  }
  .modal-body {
    overflow-y: auto;
    padding: 16px;
  }
  .modal-body p {
    margin: 0;
    color: var(--text);
    font-size: 0.9rem;
    line-height: 1.6;
    white-space: pre-wrap;
  }
  .modal-loading,
  .modal-empty {
    color: var(--text-muted) !important;
    font-size: var(--fs-sm) !important;
    margin-top: 10px !important;
  }
  .modal-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  .modal-note {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .modal-link {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--info);
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
    font-weight: 600;
    cursor: pointer;
  }
  .modal-link:hover {
    border-color: color-mix(in srgb, var(--info) 40%, var(--border));
  }
</style>
