<script lang="ts">
  let {
    modCount,
    mapsCount,
    scriptsCount,
    uniqueCount,
    activeCount,
    conflictCount,
    criticalCount,
    healthCount,
    tookMs,
    statFilter = $bindable(),
    onOpenStats,
    onOpenConflicts,
    onOpenHealth,
    onOpenLog,
    onOpenBindings,
    onOpenBridge,
  }: {
    modCount: number;
    mapsCount: number;
    scriptsCount: number;
    uniqueCount: number;
    activeCount: number;
    conflictCount: number;
    criticalCount: number;
    healthCount: number;
    tookMs: number | null;
    statFilter: "" | "maps" | "scripts" | "unique" | "active";
    onOpenStats: () => void;
    onOpenConflicts: () => void;
    onOpenHealth: () => void;
    onOpenLog: () => void;
    onOpenBindings: () => void;
    onOpenBridge: () => void;
  } = $props();
</script>

<div class="statbar">
  <button class="stat statbtn" title="Library statistics" onclick={onOpenStats}>
    <span class="stat-num tnum">{modCount}</span>
    <span class="stat-label">mods</span>
  </button>
  <button
    class="stat statbtn"
    class:sel={statFilter === "maps"}
    title="Filter to maps only"
    onclick={() => (statFilter = statFilter === "maps" ? "" : "maps")}
  >
    <span class="stat-num tnum">{mapsCount}</span>
    <span class="stat-label">maps</span>
  </button>
  <button
    class="stat statbtn"
    class:sel={statFilter === "scripts"}
    title="Filter to script mods only"
    onclick={() => (statFilter = statFilter === "scripts" ? "" : "scripts")}
  >
    <span class="stat-num tnum">{scriptsCount}</span>
    <span class="stat-label">script mods</span>
  </button>
  <button
    class="stat statbtn"
    class:sel={statFilter === "unique"}
    title="Filter to mods that declare a uniqueType"
    onclick={() => (statFilter = statFilter === "unique" ? "" : "unique")}
  >
    <span class="stat-num tnum">{uniqueCount}</span>
    <span class="stat-label">uniqueType</span>
  </button>
  <button
    class="stat statbtn"
    class:sel={statFilter === "active"}
    title="Filter to the active set only"
    onclick={() => (statFilter = statFilter === "active" ? "" : "active")}
  >
    <span class="stat-num tnum">{activeCount}</span>
    <span class="stat-label">active</span>
  </button>
  <button
    class="stat statbtn"
    class:flag={conflictCount > 0}
    class:crit={criticalCount > 0}
    title="Conflicts within the active set"
    onclick={onOpenConflicts}
  >
    <span class="stat-num tnum">{conflictCount}</span>
    <span class="stat-label">conflict{conflictCount === 1 ? "" : "s"}</span>
  </button>
  <button
    class="stat statbtn"
    class:flag={healthCount > 0}
    title="Library health: missing dependencies, corrupt mods, ignored names"
    onclick={onOpenHealth}
  >
    <span class="stat-num tnum">{healthCount}</span>
    <span class="stat-label">need attention</span>
  </button>
  <button
    class="stat statbtn"
    title="Crash & log triage: did the last run crash, and which mod is at fault?"
    onclick={onOpenLog}
  >
    <span class="stat-num">◆</span>
    <span class="stat-label">diagnose</span>
  </button>
  <button
    class="stat statbtn"
    title="The full control-binding map — every action and key, grouped by device"
    onclick={onOpenBindings}
  >
    <span class="stat-num">⌨</span>
    <span class="stat-label">bindings</span>
  </button>
  <button
    class="stat statbtn"
    title="Filltype bridge: make a stubborn map filltype work with your equipment"
    onclick={onOpenBridge}
  >
    <span class="stat-num">⛓</span>
    <span class="stat-label">bridge</span>
  </button>
  {#if tookMs !== null}
    <div class="took tnum" title="Scan wall-clock time">
      scanned in {tookMs} ms
    </div>
  {/if}
</div>

<style>
  /* Rendered as a content-width footer that floats over the scrolling library (see App). */
  .statbar {
    display: flex;
    align-items: center;
    gap: 10px 18px;
    flex-wrap: wrap;
    padding: 10px 18px;
    border-top: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface) 94%, transparent);
    backdrop-filter: saturate(1.3) blur(8px);
  }
  .stat {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .stat-num {
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    font-weight: 600;
  }
  .stat-label {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .stat.flag .stat-num {
    color: var(--warn-text);
  }
  .statbtn {
    border: none;
    background: transparent;
    cursor: pointer;
    padding: 0;
    font: inherit;
  }
  .statbtn:hover {
    opacity: 0.8;
  }
  /* A stat acting as an active filter — highlight the number + underline the label. */
  .statbtn.sel .stat-num {
    color: var(--primary-text);
  }
  .statbtn.sel .stat-label {
    color: var(--primary-text);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .statbtn.flag .stat-num {
    color: var(--warn-text);
  }
  .statbtn.crit .stat-num {
    color: var(--danger);
  }
  .took {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-left: auto;
  }
</style>
