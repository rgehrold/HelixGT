<script lang="ts">
  import { onMount, tick } from "svelte";
  import { formatLogLine, logsToText } from "$lib/log";
  import type { LogEntry, LogLevel } from "$lib/types";

  interface Props {
    logs: LogEntry[];
    toolLogs: LogEntry[];
    open?: boolean;
    onClose?: () => void;
    onClearActivity?: () => void;
    onClearTools?: () => void;
  }

  let {
    logs,
    toolLogs,
    open = false,
    onClose,
    onClearActivity,
    onClearTools,
  }: Props = $props();

  let tab = $state<"activity" | "tools">("activity");
  let filter = $state("");
  let levelFilter = $state<"all" | LogLevel>("all");
  let autoScroll = $state(true);
  let logEl = $state<HTMLDivElement | null>(null);
  let status = $state("");

  const activeEntries = $derived(tab === "activity" ? logs : toolLogs);

  const filtered = $derived.by(() => {
    const q = filter.trim().toLowerCase();
    return activeEntries.filter((entry) => {
      if (levelFilter !== "all" && entry.level !== levelFilter) return false;
      if (!q) return true;
      return entry.message.toLowerCase().includes(q);
    });
  });

  const errorCount = $derived(logs.filter((e) => e.level === "error").length);
  const warnCount = $derived(logs.filter((e) => e.level === "warn").length);

  $effect(() => {
    filtered;
    tab;
    if (!autoScroll || !logEl) return;
    void tick().then(() => {
      if (logEl) logEl.scrollTop = logEl.scrollHeight;
    });
  });

  async function copyLogs() {
    const text = logsToText(filtered);
    try {
      await navigator.clipboard.writeText(text || "(empty)");
      status = "Copied to clipboard.";
    } catch {
      status = "Could not copy.";
    }
  }

  function clearCurrent() {
    if (tab === "activity") onClearActivity?.();
    else onClearTools?.();
    status = "Cleared.";
  }

  function exportLogs() {
    const text = logsToText(filtered);
    const blob = new Blob([text || ""], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `helixgt-${tab}-log.txt`;
    a.click();
    URL.revokeObjectURL(url);
    status = "Exported.";
  }

  onMount(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && open) onClose?.();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="logs-backdrop" onclick={onClose}></div>
  <section class="logs-drawer" role="dialog" aria-label="Application logs">
    <div class="drawer-head">
      <div class="head-left">
        <h2>Logs</h2>
        {#if errorCount > 0 || warnCount > 0}
          <span class="badges">
            {#if errorCount > 0}<span class="badge error">{errorCount}</span>{/if}
            {#if warnCount > 0}<span class="badge warn">{warnCount}</span>{/if}
          </span>
        {/if}
      </div>
      <div class="log-tabs">
        <button class="log-tab" class:active={tab === "activity"} onclick={() => (tab = "activity")}>
          Activity ({logs.length})
        </button>
        <button class="log-tab" class:active={tab === "tools"} onclick={() => (tab = "tools")}>
          Tools ({toolLogs.length})
        </button>
      </div>
      <button class="ghost compact" onclick={copyLogs}>Copy</button>
      <button class="ghost compact" onclick={exportLogs}>Export</button>
      <button class="ghost compact" onclick={clearCurrent}>Clear</button>
      <button class="close-btn" onclick={onClose} aria-label="Close logs">✕</button>
    </div>

    <div class="drawer-toolbar">
      <input
        class="filter"
        type="search"
        placeholder="Filter…"
        bind:value={filter}
      />
      <select bind:value={levelFilter} aria-label="Filter by level">
        <option value="all">All</option>
        <option value="info">Info</option>
        <option value="warn">Warn</option>
        <option value="error">Error</option>
      </select>
      <label class="auto">
        <input type="checkbox" bind:checked={autoScroll} />
        Auto
      </label>
    </div>

    {#if status}
      <p class="status-msg">{status}</p>
    {/if}

    <div class="log" bind:this={logEl}>
      {#if tab === "tools" && toolLogs.length === 0}
        <div class="log-line muted">
          Tool output from minimap2 and samtools appears here during alignment and CRAM jobs.
        </div>
      {:else if filtered.length === 0}
        <div class="log-line muted">No log lines match this filter.</div>
      {:else}
        {#each filtered as entry, index (index + entry.message + (entry.time ?? ""))}
          <div
            class="log-line"
            class:log-error={entry.level === "error"}
            class:log-warn={entry.level === "warn"}
          >
            {#if entry.time}<span class="time">{entry.time}</span>{/if}
            <span class="level">{entry.level}</span>
            <span class="msg">{entry.message}</span>
          </div>
        {/each}
      {/if}
    </div>
  </section>
{/if}

<style>
  .logs-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 80;
  }

  .logs-drawer {
    position: fixed;
    left: 50%;
    bottom: 0;
    transform: translateX(-50%);
    width: min(900px, calc(100vw - 16px));
    max-height: min(42vh, 360px);
    display: flex;
    flex-direction: column;
    background: var(--panel-bg);
    border: 1px solid var(--panel-border);
    border-bottom: none;
    border-radius: 12px 12px 0 0;
    box-shadow: 0 -12px 40px rgba(0, 0, 0, 0.4);
    z-index: 90;
    padding: 8px 10px 10px;
    backdrop-filter: blur(12px);
  }

  .drawer-head {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 10px;
  }

  .head-left {
    display: flex;
    align-items: center;
    gap: 10px;
    flex: 1 1 auto;
  }

  h2 {
    margin: 0;
    font-size: 0.9rem;
  }

  .drawer-head {
    margin-bottom: 6px;
    gap: 8px;
  }

  .ghost.compact {
    padding: 3px 8px;
    font-size: 0.74rem;
    border-radius: 999px;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    color: var(--text-menu);
    cursor: pointer;
    font-weight: 600;
  }

  .filter,
  select {
    padding: 5px 8px !important;
    font-size: 0.78rem !important;
  }

  .badges {
    display: flex;
    gap: 6px;
  }

  .badge {
    font-size: 0.72rem;
    font-weight: 700;
    padding: 2px 8px;
    border-radius: 999px;
  }

  .badge.error {
    color: var(--status-error-text);
    background: var(--status-error-bg);
    border: 1px solid var(--status-error-border);
  }

  .badge.warn {
    color: var(--warn);
    background: var(--chip-bg);
    border: 1px solid var(--chip-border);
  }

  .log-tabs {
    display: flex;
    gap: 6px;
  }

  .log-tab,
  .ghost,
  .close-btn {
    cursor: pointer;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    color: var(--text-menu);
    border-radius: 999px;
    font-weight: 600;
  }

  .log-tab {
    padding: 4px 10px;
    font-size: 0.78rem;
  }

  .log-tab.active {
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
    background: var(--chip-active-bg);
  }

  .close-btn {
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: 10px;
  }

  .drawer-toolbar {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin-bottom: 8px;
  }

  .filter,
  select {
    padding: 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    font-size: 0.82rem;
  }

  .filter {
    flex: 1 1 160px;
    min-width: 140px;
  }

  .auto {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 0.8rem;
  }

  .ghost {
    padding: 6px 12px;
    font-size: 0.78rem;
  }

  .status-msg {
    margin: 0 0 6px;
    font-size: 0.78rem;
    color: var(--text-muted);
  }

  .log {
    flex: 1 1 auto;
    min-height: 140px;
    overflow: auto;
    padding: 12px;
    border-radius: 12px;
    background: var(--log-surface-bg);
    border: 1px solid var(--log-surface-border);
    color: var(--log-text);
    font-family: "JetBrains Mono", monospace;
    font-size: 0.76rem;
    line-height: 1.5;
  }

  .log-line {
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: 8px;
    margin-bottom: 3px;
    white-space: pre-wrap;
  }

  .log-line.muted {
    display: block;
    color: var(--text-muted);
  }

  .time {
    color: var(--text-muted);
    opacity: 0.85;
  }

  .level {
    text-transform: uppercase;
    font-size: 0.68rem;
    font-weight: 700;
    color: var(--text-muted);
    min-width: 36px;
  }

  .log-error,
  .log-error .level {
    color: var(--error);
  }

  .log-warn,
  .log-warn .level {
    color: var(--warn);
  }
</style>
