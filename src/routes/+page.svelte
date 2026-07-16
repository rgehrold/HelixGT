<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";

  import { onMount } from "svelte";
  import AlignPane from "$lib/components/AlignPane.svelte";
  import AnalyzePane from "$lib/components/AnalyzePane.svelte";
  import ConvertPane from "$lib/components/ConvertPane.svelte";
  import FileExplorer from "$lib/components/FileExplorer.svelte";
  import MergePane from "$lib/components/MergePane.svelte";
  import LogsPanel from "$lib/components/LogsPanel.svelte";
  import ManualPanel from "$lib/components/ManualPanel.svelte";
  import MenuBar from "$lib/components/MenuBar.svelte";
  import ViewPane from "$lib/components/ViewPane.svelte";
  import { appendLog } from "$lib/log";
  import { getSupportedFormats } from "$lib/api";
  import {
    clampFilesPaneWidth,
    DEFAULT_EXPANDED_FILES_PANE_PERCENT,
    shouldSnapCollapseFilesPane,
  } from "$lib/layout";
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
  let logsOpen = $state(false);
  let fileExplorer = $state<{
    revealPaths: (paths: string[]) => Promise<void>;
    refreshTree: () => Promise<void>;
  } | null>(null);
  let alignPane = $state<{
    loadReferenceFromPath: (path: string) => Promise<void>;
  } | null>(null);
  let filesPaneWidth = $state(defaultFilesPaneWidth());
  let filesPaneCollapsed = $state(false);
  let filesPaneWidthBeforeCollapse = $state(defaultFilesPaneWidth());
  let isResizing = $state(false);
  let activeMode = $state<ToolMode>("convert");
  let isMerging = $state(false);
  let isAligning = $state(false);
  let isBusy = $derived(isConverting || isMerging || isAligning);
  let viewOpenPath = $state<string | null>(null);

  const selectedHasCram = $derived(selectedPaths.some((path) => /\.cram$/i.test(path)));

  const modeMeta: Record<ToolMode, { title: string }> = {
    convert: { title: "Convert" },
    merge: { title: "Merge" },
    align: { title: "Align" },
    view: { title: "View" },
    analyze: { title: "File analysis" },
  };

  const needsReferenceForConvert = $derived(
    activeMode === "convert" && (outputFormat === "cram" || selectedHasCram),
  );

  const showSetAsReference = $derived(
    activeMode === "align" ||
      activeMode === "view" ||
      activeMode === "analyze" ||
      needsReferenceForConvert,
  );

  $effect(() => {
    rememberMode(activeMode);
  });

  $effect(() => {
    if (outputDir.trim()) rememberOutputDir(activeMode, outputDir);
  });

  // Open logs automatically when a new error arrives during a job.
  let lastErrorLen = 0;
  $effect(() => {
    const errors = logs.filter((e) => e.level === "error").length;
    if (errors > lastErrorLen && isBusy) {
      logsOpen = true;
    }
    lastErrorLen = errors;
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
          filesPaneWidthBeforeCollapse = prefs.filesPaneWidth;
        }
        if (typeof prefs.filesPaneCollapsed === "boolean") {
          filesPaneCollapsed = prefs.filesPaneCollapsed;
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

  function clearActivityLogs() {
    logs = [];
  }

  function clearToolLogs() {
    toolLogs = [];
  }

  function toggleLogs() {
    logsOpen = !logsOpen;
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
    if (activeMode === "align") {
      await alignPane?.loadReferenceFromPath(path);
      return;
    }
    // Convert, View, and File analysis share the CRAM reference FASTA path.
    if (activeMode === "convert" || activeMode === "view" || activeMode === "analyze") {
      referencePath = path;
      pushLog(`Reference set to ${path}`);
    }
  }

  function restoreStandardView() {
    filesPaneCollapsed = false;
    filesPaneWidth = defaultFilesPaneWidth();
    filesPaneWidthBeforeCollapse = filesPaneWidth;
    patchUserPreferences({
      filesPaneWidth: filesPaneWidth,
      filesPaneCollapsed: false,
    });
    activeMode = "convert";
    logsOpen = false;
    void fileExplorer?.refreshTree();
    pushLog("Restored standard view and refreshed file browser.");
  }

  function collapseFilesPane() {
    if (!filesPaneCollapsed) {
      filesPaneWidthBeforeCollapse = filesPaneWidth;
    }
    filesPaneCollapsed = true;
    patchUserPreferences({ filesPaneCollapsed: true, filesPaneWidth: filesPaneWidthBeforeCollapse });
    saveUserPreferencesNow();
  }

  function expandFilesPane() {
    filesPaneCollapsed = false;
    filesPaneWidth = Math.max(
      DEFAULT_EXPANDED_FILES_PANE_PERCENT,
      filesPaneWidthBeforeCollapse || defaultFilesPaneWidth(),
    );
    patchUserPreferences({ filesPaneCollapsed: false, filesPaneWidth });
    saveUserPreferencesNow();
    queueMicrotask(() => syncPaneWidthToWorkspace(document.querySelector(".workspace")));
  }

  function toggleFilesPane() {
    if (filesPaneCollapsed) expandFilesPane();
    else collapseFilesPane();
  }

  function syncPaneWidthToWorkspace(workspace: Element | null) {
    if (!workspace || filesPaneCollapsed) return;
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
    let snapCollapse = false;

    const onMove = (moveEvent: MouseEvent) => {
      const rect = workspace.getBoundingClientRect();
      if (shouldSnapCollapseFilesPane(moveEvent.clientX, rect.left)) {
        snapCollapse = true;
        filesPaneWidth = clampFilesPaneWidth(
          (80 / rect.width) * 100,
          rect.width,
        );
        return;
      }
      snapCollapse = false;
      const next = ((moveEvent.clientX - rect.left) / rect.width) * 100;
      filesPaneWidth = clampFilesPaneWidth(next, rect.width);
    };

    const onUp = () => {
      isResizing = false;
      if (snapCollapse) {
        collapseFilesPane();
      } else {
        filesPaneWidthBeforeCollapse = filesPaneWidth;
        patchUserPreferences({ filesPaneWidth, filesPaneCollapsed: false });
        saveUserPreferencesNow();
      }
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
    if (mode !== "view") {
      void loadUserPreferences().then((prefs) => {
        outputDir = outputDirForMode(prefs, mode);
      });
    }
  }

  function openInView(path: string) {
    activeMode = "view";
    viewOpenPath = path;
    pushLog(`Opening in View: ${path}`);
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="app" oncontextmenu={handleAppContextMenu}>
  <MenuBar
    onPrint={() => window.print()}
    onRestoreView={restoreStandardView}
    onToggleLogs={toggleLogs}
    onToggleFilesBrowser={toggleFilesPane}
    filesBrowserCollapsed={filesPaneCollapsed}
    {logsOpen}
  />

  <main
    class="workspace"
    class:resizing={isResizing}
    class:files-collapsed={filesPaneCollapsed}
    style={`--files-width:${filesPaneWidth}%`}
  >
    {#if filesPaneCollapsed}
      <button
        type="button"
        class="files-stash"
        title="Show input browser"
        aria-label="Show input browser"
        onclick={expandFilesPane}
      >
        <span class="stash-label">Files</span>
      </button>
    {:else}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <section class="panel files-panel" class:dragging={isDragging} ondragover={onDragOver}>
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
          onOpenInView={(path) => openInView(path)}
        />
      </section>

      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div
        class="resize-handle"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize panes — drag left to hide browser"
        title="Drag left to hide browser"
        onmousedown={startPaneResize}
      ></div>
    {/if}

    <div class="right-column">
      <section class="panel settings-panel">
        <nav class="mode-rail" aria-label="Tool modes">
          <button
            type="button"
            class="mode-tab"
            class:active={activeMode === "convert"}
            aria-current={activeMode === "convert" ? "page" : undefined}
            onclick={() => switchMode("convert")}
          >
            <span class="mode-tab-title">Convert</span>
          </button>
          <button
            type="button"
            class="mode-tab"
            class:active={activeMode === "merge"}
            aria-current={activeMode === "merge" ? "page" : undefined}
            onclick={() => switchMode("merge")}
          >
            <span class="mode-tab-title">Merge</span>
          </button>
          <button
            type="button"
            class="mode-tab"
            class:active={activeMode === "align"}
            aria-current={activeMode === "align" ? "page" : undefined}
            onclick={() => switchMode("align")}
          >
            <span class="mode-tab-title">Align</span>
          </button>
          <button
            type="button"
            class="mode-tab"
            class:active={activeMode === "view"}
            aria-current={activeMode === "view" ? "page" : undefined}
            onclick={() => switchMode("view")}
          >
            <span class="mode-tab-title">View</span>
          </button>
          <button
            type="button"
            class="mode-tab"
            class:active={activeMode === "analyze"}
            aria-current={activeMode === "analyze" ? "page" : undefined}
            onclick={() => switchMode("analyze")}
          >
            <span class="mode-tab-title">Analyze</span>
          </button>
        </nav>

        <div class="settings-body">
          <div class="settings-scroll" class:view-scroll={activeMode === "view"}>
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
            {:else if activeMode === "align"}
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
            {:else if activeMode === "view"}
              <ViewPane
                {selectedPaths}
                openPathRequest={viewOpenPath}
                bind:referencePath
                disabled={isBusy}
                onLog={pushLog}
                onOpenPathConsumed={() => (viewOpenPath = null)}
              />
            {:else}
              <AnalyzePane
                {selectedPaths}
                bind:referencePath
                disabled={isBusy}
                onLog={pushLog}
              />
            {/if}
          </div>
        </div>
      </section>
    </div>
  </main>

  <LogsPanel
    {logs}
    {toolLogs}
    open={logsOpen}
    onClose={() => (logsOpen = false)}
    onClearActivity={clearActivityLogs}
    onClearTools={clearToolLogs}
  />
  <ManualPanel />
</div>

<style>
  .app {
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 6px 10px 8px;
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
    width: var(--files-width, 28%);
    min-width: 140px;
    max-width: 85%;
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex-shrink: 0;
    overflow: hidden;
    padding: 8px 8px 8px !important;
  }

  .files-panel.dragging {
    outline: 1px dashed var(--drag-outline);
    outline-offset: -1px;
    background: var(--drag-bg);
  }

  .files-panel :global(.explorer) {
    flex: 1 1 auto;
    min-height: 0;
  }

  .files-stash {
    flex: 0 0 28px;
    width: 28px;
    margin-right: 4px;
    border-radius: 10px;
    border: 1px solid var(--panel-border);
    background: var(--panel-bg);
    color: var(--text-menu);
    cursor: pointer;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    writing-mode: vertical-rl;
    text-orientation: mixed;
    font: inherit;
    font-size: 0.72rem;
    font-weight: 650;
    letter-spacing: 0.06em;
    box-shadow: var(--panel-shadow);
  }

  .files-stash:hover {
    color: var(--menu-active-text);
    background: var(--menu-hover-bg);
    border-color: var(--chip-active-border);
  }

  .stash-label {
    transform: rotate(180deg);
    padding: 8px 0;
  }

  .resize-handle {
    flex: 0 0 6px;
    width: 6px;
    min-width: 6px;
    margin: 0 4px;
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
    min-width: 240px;
    display: flex;
    flex-direction: column;
    gap: 0;
    min-height: 0;
    overflow: hidden;
  }

  .right-column > .settings-panel {
    flex: 1 1 auto;
  }

  .panel {
    background: var(--panel-bg);
    border: 1px solid var(--panel-border);
    border-radius: 12px;
    padding: 10px 12px;
    backdrop-filter: blur(10px);
    box-shadow: var(--panel-shadow);
    min-height: 0;
  }

  .settings-panel {
    flex: 1 1 auto;
    display: flex;
    flex-direction: row;
    align-items: stretch;
    overflow: hidden;
    min-height: 0;
    padding: 0;
  }

  .mode-rail {
    flex: 0 0 84px;
    width: 84px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 8px 6px;
    border-right: 1px solid var(--panel-border);
    background: color-mix(in srgb, var(--tree-bg) 80%, transparent);
  }

  .mode-tab {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0;
    width: 100%;
    text-align: left;
    cursor: pointer;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-menu);
    padding: 7px 8px;
    border-radius: 8px;
    font: inherit;
  }

  .mode-tab:hover {
    background: var(--menu-hover-bg);
    color: var(--menu-active-text);
  }

  .mode-tab.active {
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
    background: var(--chip-active-bg);
    box-shadow: inset 3px 0 0 var(--accent-highlight);
  }

  .mode-tab-title {
    font-size: 0.78rem;
    font-weight: 650;
    line-height: 1.15;
  }

  .settings-body {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 8px 10px 10px;
  }

  .settings-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding-right: 2px;
  }

  .settings-scroll.view-scroll {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  h2 {
    margin: 0;
    font-size: 0.95rem;
    letter-spacing: 0.02em;
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
    align-items: stretch;
    gap: 8px;
    min-width: 0;
  }

  .row > .ghost {
    flex: 0 0 auto;
    align-self: stretch;
  }

  .row > input:not([type="checkbox"]) {
    flex: 1 1 auto;
    min-width: 0;
    width: auto;
  }

  input:not([type="checkbox"]) {
    width: 100%;
    padding: 11px 12px;
    border-radius: 12px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    box-sizing: border-box;
  }

  input:not([type="checkbox"]):focus,
  input:not([type="checkbox"]):focus-visible {
    outline: none;
    border-color: var(--accent-highlight);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-highlight) 55%, transparent);
    position: relative;
    z-index: 1;
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