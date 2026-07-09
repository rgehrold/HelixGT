<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";

  import { onMount } from "svelte";
  import AlignPane from "$lib/components/AlignPane.svelte";
  import ConvertPane from "$lib/components/ConvertPane.svelte";
  import FileExplorer from "$lib/components/FileExplorer.svelte";
  import MergePane from "$lib/components/MergePane.svelte";
  import MenuBar from "$lib/components/MenuBar.svelte";
  import { appendLog } from "$lib/log";
  import { getSupportedFormats } from "$lib/api";
  import { clampFilesPaneWidth } from "$lib/layout";
  import {
    defaultFilesPaneWidth,
    loadUserPreferences,
    outputDirForMode,
    patchUserPreferences,
    rememberMode,
    rememberOutputDir,
    saveUserPreferencesNow,
  } from "$lib/userPreferences";
  import type {
    ConvertSummary,
    FormatInfo,
    LogEntry,
    LogLevel,
    ToolLogEvent,
    ToolMode,
  } from "$lib/types";

  let formats = $state<FormatInfo[]>([]);
  let selectedPaths = $state<string[]>([]);
  let outputDir = $state("");
  let outputFormat = $state("fasta");
  let compress = $state(true);
  let prefix = $state("");
  let referencePath = $state("");
  let minimap2Available = $state(false);
  let samtoolsAvailable = $state(false);
  let isConverting = $state(false);
  let isDragging = $state(false);
  let logs = $state<LogEntry[]>([]);
  let toolLogs = $state<LogEntry[]>([]);
  let logTab = $state<"activity" | "tools">("activity");
  let fileExplorer = $state<{
    revealPaths: (paths: string[]) => Promise<void>;
    refreshTree: () => Promise<void>;
  } | null>(null);
  let alignPane = $state<{
    loadReferenceFromPath: (path: string) => Promise<void>;
  } | null>(null);
  let filesPaneWidth = $state(58);
  let isResizing = $state(false);
  let logEl = $state<HTMLDivElement | null>(null);
  let activeMode = $state<ToolMode>("convert");
  let isMerging = $state(false);
  let isAligning = $state(false);
  let isBusy = $derived(isConverting || isMerging || isAligning);

  $effect(() => {
    if (isAligning) logTab = "tools";
  });

  const selectedHasCram = $derived(selectedPaths.some((path) => /\.cram$/i.test(path)));

  const needsReferenceForConvert = $derived(
    activeMode === "convert" && (outputFormat === "cram" || selectedHasCram),
  );

  const showSetAsReference = $derived(activeMode === "align" || needsReferenceForConvert);

  $effect(() => {
    rememberMode(activeMode);
  });

  $effect(() => {
    if (outputDir.trim()) rememberOutputDir(activeMode, outputDir);
  });

  $effect(() => {
    logs;
    toolLogs;
    logTab;
    if (logEl) {
      logEl.scrollTop = logEl.scrollHeight;
    }
  });

  onMount(() => {
    const onWindowResize = () => {
      syncPaneWidthToWorkspace(document.querySelector(".workspace"));
    };
    window.addEventListener("resize", onWindowResize);

    void (async () => {
      try {
        const prefs = await loadUserPreferences();
        if (typeof prefs.filesPaneWidth === "number") {
          filesPaneWidth = prefs.filesPaneWidth;
        }
        if (prefs.activeMode) activeMode = prefs.activeMode;
        if (prefs.outputFormat) outputFormat = prefs.outputFormat;
        outputDir = outputDirForMode(prefs, activeMode);
        queueMicrotask(() => syncPaneWidthToWorkspace(document.querySelector(".workspace")));
      } catch (error) {
        pushLog(`Could not load saved layout: ${String(error)}`, "warn");
      }

      pushLog("Ready.");
      try {
        formats = await getSupportedFormats();
      } catch (error) {
        pushLog(`Could not load output formats: ${String(error)}`, "error");
      }
      minimap2Available = await invoke<boolean>("minimap2_is_available");
      samtoolsAvailable = await invoke<boolean>("samtools_is_available");

      await listen<ToolLogEvent>("tool-log", (event) => {
        const { tool, stream, line } = event.payload;
        pushToolLog(`[${tool} ${stream}] ${line}`);
      });
      await getCurrentWebview().onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          isDragging = false;
          void addPaths(event.payload.paths);
        } else if (event.payload.type === "over") {
          isDragging = true;
        } else {
          isDragging = false;
        }
      });
    })();

    return () => {
      window.removeEventListener("resize", onWindowResize);
    };
  });

  function pushLog(message: string, level?: LogLevel) {
    logs = appendLog(logs, message, level);
  }

  function pushToolLog(message: string, level?: LogLevel) {
    toolLogs = appendLog(toolLogs, message, level);
  }

  function alignmentIndexPath(outputPath: string): string | null {
    if (outputPath.endsWith(".bam")) return `${outputPath}.bai`;
    if (outputPath.endsWith(".cram")) return `${outputPath}.crai`;
    return null;
  }

  function collectOutputPaths(paths: string[]) {
    const all = [...paths];
    for (const path of paths) {
      const indexPath = alignmentIndexPath(path);
      if (indexPath) all.push(indexPath);
    }
    return all;
  }

  async function setReferenceFromBrowser(path: string) {
    if (activeMode === "convert") {
      referencePath = path;
      pushLog(`Reference set to ${path}`);
      return;
    }
    if (activeMode === "align") {
      await alignPane?.loadReferenceFromPath(path);
    }
  }

  function restoreStandardView() {
    filesPaneWidth = defaultFilesPaneWidth();
    patchUserPreferences({ filesPaneWidth: filesPaneWidth });
    activeMode = "convert";
    logTab = "activity";
    void fileExplorer?.refreshTree();
    pushLog("Restored standard view and refreshed file browser.");
  }

  function syncPaneWidthToWorkspace(workspace: Element | null) {
    if (!workspace) return;
    const rect = workspace.getBoundingClientRect();
    const clamped = clampFilesPaneWidth(filesPaneWidth, rect.width);
    if (clamped !== filesPaneWidth) {
      filesPaneWidth = clamped;
      patchUserPreferences({ filesPaneWidth: clamped });
    }
  }

  function handleAppContextMenu(event: MouseEvent) {
    if (!(event.target as HTMLElement).closest(".explorer")) {
      event.preventDefault();
    }
  }

  async function addPaths(paths: string[]) {
    if (paths.length === 0) return;
    const expanded: string[] = [];
    for (const path of paths) {
      try {
        const children = await invoke<string[]>("collect_compatible_files_under", { path });
        if (children.length > 0) {
          expanded.push(...children);
          continue;
        }
      } catch {
        // not a folder or unreadable — treat as file
      }
      expanded.push(path);
    }
    const valid = await invoke<string[]>("validate_input_paths", { paths: expanded });
    const merged = new Set([...selectedPaths, ...valid]);
    selectedPaths = [...merged].sort();
    if (valid.length < paths.length) {
      pushLog(`Skipped ${paths.length - valid.length} unsupported file(s).`, "warn");
    }
    if (valid.length > 0) {
      pushLog(`Added ${valid.length} file(s) from drag and drop.`);
      await fileExplorer?.revealPaths(valid);
    }
  }

  function startPaneResize(event: MouseEvent) {
    event.preventDefault();
    isResizing = true;
    const workspace = (event.currentTarget as HTMLElement).closest(".workspace");
    if (!workspace) return;

    const onMove = (moveEvent: MouseEvent) => {
      const rect = workspace.getBoundingClientRect();
      const next = ((moveEvent.clientX - rect.left) / rect.width) * 100;
      filesPaneWidth = clampFilesPaneWidth(next, rect.width);
    };

    const onUp = () => {
      isResizing = false;
      patchUserPreferences({ filesPaneWidth });
      saveUserPreferencesNow();
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  function onDragOver(event: DragEvent) {
    event.preventDefault();
  }

  function switchMode(mode: ToolMode) {
    activeMode = mode;
    void loadUserPreferences().then((prefs) => {
      outputDir = outputDirForMode(prefs, mode);
    });
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="app" oncontextmenu={handleAppContextMenu}>
  <MenuBar onPrint={() => window.print()} onRestoreView={restoreStandardView} />

  <main class="workspace" class:resizing={isResizing} style={`--files-width:${filesPaneWidth}%`}>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <section class="panel files-panel" class:dragging={isDragging} ondragover={onDragOver}>
      <div class="panel-head">
        <h2>Input browser</h2>
        <span class="panel-hint">Drop files here · right-click for actions</span>
      </div>

      <FileExplorer
        bind:this={fileExplorer}
        bind:selectedPaths
        disabled={isBusy}
        {showSetAsReference}
        onSetOutputFolder={(path) => {
          outputDir = path;
          pushLog(`Output folder set to ${path}`);
        }}
        onSetAsReference={(path) => void setReferenceFromBrowser(path)}
      />
    </section>

    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="resize-handle"
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize panes"
      onmousedown={startPaneResize}
    ></div>

    <div class="right-column">
    <section class="panel settings-panel">
      <div class="panel-head">
        <h2>Tools</h2>
        <div class="mode-tabs">
          <button class="mode-tab" class:active={activeMode === "convert"} onclick={() => switchMode("convert")}>
            Convert
          </button>
          <button class="mode-tab" class:active={activeMode === "merge"} onclick={() => switchMode("merge")}>
            Merge
          </button>
          <button class="mode-tab" class:active={activeMode === "align"} onclick={() => switchMode("align")}>
            Align
          </button>
        </div>
      </div>

      <div class="settings-scroll">
      {#if activeMode === "convert"}
        <ConvertPane
          {formats}
          {selectedPaths}
          {outputDir}
          bind:compress
          bind:prefix
          bind:referencePath
          outputFormat={outputFormat}
          disabled={isBusy}
          {samtoolsAvailable}
          onOutputFormatChange={(value) => {
            outputFormat = value;
            patchUserPreferences({ outputFormat: value });
          }}
          onOutputDirChange={(value) => (outputDir = value)}
          onLog={pushLog}
          onBusyChange={(busy) => (isConverting = busy)}
          onComplete={async (summary) => {
            await fileExplorer?.refreshTree();
            const outputPaths = collectOutputPaths(summary.files.map((file) => file.outputPath));
            if (outputPaths.length > 0) await fileExplorer?.revealPaths(outputPaths);
          }}
        />
      {:else if activeMode === "merge"}
        <MergePane
          {selectedPaths}
          {outputDir}
          disabled={isBusy}
          onOutputDirChange={(value) => (outputDir = value)}
          onLog={pushLog}
          onBusyChange={(busy) => (isMerging = busy)}
          onComplete={async (outputPath) => {
            await fileExplorer?.refreshTree();
            await fileExplorer?.revealPaths([outputPath]);
          }}
        />
      {:else}
        <AlignPane
          bind:this={alignPane}
          {selectedPaths}
          {outputDir}
          disabled={isBusy}
          {minimap2Available}
          {samtoolsAvailable}
          onOutputDirChange={(value) => (outputDir = value)}
          onLog={pushLog}
          onBusyChange={(busy) => (isAligning = busy)}
          onComplete={async (outputPaths) => {
            await fileExplorer?.refreshTree();
            await fileExplorer?.revealPaths(collectOutputPaths(outputPaths));
          }}
        />
      {/if}
      </div>
    </section>

    <section class="panel log-panel">
      <div class="panel-head">
        <h2>Logs</h2>
        <div class="log-tabs">
          <button class="log-tab" class:active={logTab === "activity"} onclick={() => (logTab = "activity")}>
            Activity
          </button>
          <button class="log-tab" class:active={logTab === "tools"} onclick={() => (logTab = "tools")}>
            Tools
          </button>
        </div>
      </div>
      <div class="log" bind:this={logEl}>
        {#if logTab === "activity"}
          {#each logs as entry}
            <div class="log-line" class:log-error={entry.level === "error"} class:log-warn={entry.level === "warn"}>
              {entry.message}
            </div>
          {/each}
        {:else if toolLogs.length === 0}
          <div class="log-line">Tool output from minimap2 and samtools appears here during alignment.</div>
        {:else}
          {#each toolLogs as entry}
            <div class="log-line" class:log-error={entry.level === "error"} class:log-warn={entry.level === "warn"}>
              {entry.message}
            </div>
          {/each}
        {/if}
      </div>
    </section>
    </div>
  </main>
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 12px 20px 14px;
    background:
      radial-gradient(circle at top left, var(--bg-glow-1), transparent 28%),
      radial-gradient(circle at 85% 10%, var(--bg-glow-2), transparent 24%),
      var(--bg-base);
  }

  .workspace {
    display: flex;
    align-items: stretch;
    gap: 0;
    flex: 1;
    min-height: 0;
    overflow: hidden;
  }

  .workspace.resizing {
    cursor: col-resize;
    user-select: none;
  }

  .files-panel {
    width: var(--files-width, 58%);
    min-width: 300px;
    max-width: 78%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex-shrink: 0;
    overflow: hidden;
    padding-bottom: 12px;
  }

  .files-panel .panel-head {
    flex-shrink: 0;
  }

  .files-panel.dragging {
    outline: 1px dashed var(--drag-outline);
    outline-offset: -1px;
    background: var(--drag-bg);
  }

  .panel-hint {
    color: var(--text-muted);
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .files-panel :global(.explorer) {
    flex: 1 1 auto;
    min-height: 0;
  }

  .resize-handle {
    flex: 0 0 10px;
    width: 10px;
    min-width: 10px;
    margin: 0 6px;
    border-radius: 999px;
    cursor: col-resize;
    background: var(--resize-bg);
    border: 1px solid var(--resize-border);
    align-self: stretch;
    transition: background 0.15s ease, border-color 0.15s ease;
  }

  .resize-handle:hover,
  .workspace.resizing .resize-handle {
    background: var(--resize-hover-bg);
    border-color: var(--resize-hover-border);
  }

  .right-column {
    flex: 1;
    min-width: 280px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow: hidden;
  }

  .panel {
    background: var(--panel-bg);
    border: 1px solid var(--panel-border);
    border-radius: 20px;
    padding: 18px;
    backdrop-filter: blur(10px);
    box-shadow: var(--panel-shadow);
    min-height: 0;
  }

  .settings-panel {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .settings-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding-right: 2px;
  }

  .log-panel {
    flex: 0 1 34%;
    min-height: 140px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-bottom: 14px;
    flex-shrink: 0;
  }

  h2 {
    margin: 0;
    font-size: 1rem;
    letter-spacing: 0.02em;
  }

  .mode-tabs {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .mode-tab {
    cursor: pointer;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    color: var(--text-menu);
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 0.82rem;
    font-weight: 600;
  }

  .mode-tab.active {
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
    background: var(--chip-active-bg);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .field > span,
  .toggle span,
  .label-with-help {
    color: var(--text-menu);
    font-size: 0.92rem;
    font-weight: 500;
  }

  .label-with-help {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .subtle {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .row {
    display: flex;
    gap: 8px;
  }

  input:not([type="checkbox"]) {
    width: 100%;
    padding: 11px 12px;
    border-radius: 12px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
  }

  .format-groups {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .format-group p {
    margin: 0 0 6px;
    color: var(--text-muted);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .pill,
  .ghost,
  .primary {
    cursor: pointer;
    border: none;
  }

  .pill {
    padding: 8px 12px;
    border-radius: 999px;
    background: var(--chip-bg);
    color: var(--text-menu);
    border: 1px solid var(--chip-border);
    transition: all 0.15s ease;
  }

  .pill.active {
    background: var(--chip-active-bg);
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
  }

  .ghost,
  .primary {
    padding: 10px 14px;
    border-radius: 12px;
    font-weight: 600;
  }

  .ghost {
    background: var(--chip-bg);
    color: var(--text-primary);
    border: 1px solid var(--chip-border);
  }

  .primary {
    width: 100%;
    margin-top: 8px;
    background: var(--primary-bg);
    color: var(--primary-text);
    box-shadow: var(--primary-shadow);
  }

  .primary:disabled,
  .ghost:disabled,
  .pill:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
    flex-wrap: wrap;
  }

  .log-tabs {
    display: flex;
    gap: 6px;
  }

  .log-tab {
    cursor: pointer;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
    color: var(--text-menu);
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 0.78rem;
    font-weight: 600;
  }

  .log-tab.active {
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
    background: var(--chip-active-bg);
  }

  .progress-wrap {
    margin-top: 14px;
    height: 8px;
    border-radius: 999px;
    background: var(--progress-track);
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    border-radius: inherit;
    background: var(--progress-fill);
    transition: width 0.2s ease;
  }

  .progress-label,
  .success p {
    margin: 8px 0 0;
    font-size: 0.9rem;
  }

  .success {
    margin-top: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--success-bg);
    border: 1px solid var(--success-border);
  }

  .log {
    margin: 0;
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: 14px;
    border-radius: 14px;
    background: var(--log-surface-bg);
    border: 1px solid var(--log-surface-border);
    color: var(--log-text);
    font-family: "JetBrains Mono", monospace;
    font-size: 0.78rem;
    line-height: 1.5;
  }

  .log-line {
    white-space: pre-wrap;
    margin-bottom: 2px;
  }

  .log-line.log-error {
    color: var(--error);
  }

  .log-line.log-warn {
    color: var(--warn);
  }

  @media (max-width: 980px) {
    .app {
      height: auto;
      min-height: 100vh;
      overflow: auto;
    }

    .workspace {
      flex-direction: column;
      min-height: auto;
      overflow: visible;
    }

    .files-panel {
      width: 100%;
      max-width: none;
      min-height: 360px;
      flex: 1 1 50vh;
    }

    .resize-handle {
      display: none;
    }

    .right-column {
      min-width: 0;
    }

  }
</style>