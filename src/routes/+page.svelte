<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import AlignPane from "$lib/components/AlignPane.svelte";
  import FileExplorer from "$lib/components/FileExplorer.svelte";
  import MergePane from "$lib/components/MergePane.svelte";
  import MenuBar from "$lib/components/MenuBar.svelte";
  import { inferLogLevel } from "$lib/log";
  import HelpTip from "$lib/components/HelpTip.svelte";
  import type {
    ConvertProgress,
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
  let progress = $state<ConvertProgress | null>(null);
  let logs = $state<LogEntry[]>([]);
  let toolLogs = $state<LogEntry[]>([]);
  let logTab = $state<"activity" | "tools">("activity");
  let lastSummary = $state<ConvertSummary | null>(null);
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

  const categoryLabels: Record<string, string> = {
    sequence: "Sequence",
    alignment: "Alignment",
    annotation: "Annotation",
    variants: "Variants",
  };

  $effect(() => {
    logs;
    toolLogs;
    logTab;
    if (logEl) {
      logEl.scrollTop = logEl.scrollHeight;
    }
  });

  onMount(async () => {
    pushLog("Ready.");
    formats = await invoke<FormatInfo[]>("get_supported_formats");
    minimap2Available = await invoke<boolean>("minimap2_is_available");
    samtoolsAvailable = await invoke<boolean>("samtools_is_available");

    await listen<ToolLogEvent>("tool-log", (event) => {
      const { tool, stream, line } = event.payload;
      pushToolLog(`[${tool} ${stream}] ${line}`);
    });
    await listen<ConvertProgress>("convert-progress", (event) => {
      progress = event.payload;
      logs = [
        ...logs,
        {
          level: "info" as const,
          message: `[${event.payload.current}/${event.payload.total}] ${event.payload.fileName}`,
        },
      ];
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
  });

  function pushLog(message: string, level?: LogLevel) {
    logs = [...logs, { level: level ?? inferLogLevel(message), message }];
  }

  function pushToolLog(message: string, level?: LogLevel) {
    toolLogs = [...toolLogs, { level: level ?? inferLogLevel(message), message }];
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
    filesPaneWidth = 58;
    activeMode = "convert";
    logTab = "activity";
    void fileExplorer?.refreshTree();
    pushLog("Restored standard view and refreshed file browser.");
  }

  function handleAppContextMenu(event: MouseEvent) {
    if (!(event.target as HTMLElement).closest(".explorer")) {
      event.preventDefault();
    }
  }

  async function addPaths(paths: string[]) {
    if (paths.length === 0) return;
    const valid = await invoke<string[]>("validate_input_paths", { paths });
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
      filesPaneWidth = Math.min(78, Math.max(32, next));
    };

    const onUp = () => {
      isResizing = false;
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  async function browseOutputDir() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose output folder",
    });
    if (picked) outputDir = String(picked);
  }

  async function browseReferenceFasta() {
    const picked = await open({
      directory: false,
      multiple: false,
      title: "Choose reference FASTA for CRAM",
      filters: [{ name: "FASTA", extensions: ["fasta", "fa", "fna", "gz"] }],
    });
    if (picked) referencePath = String(picked);
  }

  const alignmentInputPaths = $derived(
    selectedPaths.filter((path) => /\.(sam|bam|cram)(\.gz)?$/i.test(path)),
  );

  function onDragOver(event: DragEvent) {
    event.preventDefault();
  }

  async function startConversion() {
    lastSummary = null;

    if (selectedPaths.length === 0) {
      pushLog("Select at least one compatible input file in the browser.", "error");
      return;
    }
    if (!outputDir.trim()) {
      pushLog("Choose an output folder.", "error");
      return;
    }

    const inputPaths =
      outputFormat === "cram"
        ? alignmentInputPaths
        : selectedPaths;

    if (needsReferenceForConvert) {
      if (!referencePath.trim()) {
        pushLog("Choose a reference FASTA for CRAM conversion or decoding.", "error");
        return;
      }
      if (!samtoolsAvailable) {
        pushLog("CRAM conversion requires samtools on PATH or in src-tauri/binaries/.", "error");
        return;
      }
    }

    if (outputFormat === "cram") {
      if (inputPaths.length === 0) {
        pushLog("Select at least one SAM or BAM file to convert to CRAM.", "error");
        return;
      }
      if (inputPaths.length < selectedPaths.length) {
        pushLog(
          `Converting ${inputPaths.length} alignment file(s); skipped ${selectedPaths.length - inputPaths.length} non-alignment file(s).`,
          "warn",
        );
      }
    } else if (inputPaths.length === 0) {
      pushLog("Select at least one compatible input file in the browser.", "error");
      return;
    }

    isConverting = true;
    progress = null;
    pushLog("Starting batch conversion…");

    try {
      const summary = await invoke<ConvertSummary>("run_conversion", {
        request: {
          inputPaths,
          outputDir: outputDir.trim(),
          outputFormat,
          compress,
          prefix: prefix.trim() || null,
          referencePath: needsReferenceForConvert ? referencePath.trim() : null,
        },
      });
      lastSummary = summary;
      pushLog(
        `Done — ${summary.files.length} file(s), ${summary.totalRecords.toLocaleString()} records.`,
      );
      await fileExplorer?.refreshTree();
      const outputPaths = collectOutputPaths(summary.files.map((file) => file.outputPath));
      if (outputPaths.length > 0) {
        await fileExplorer?.revealPaths(outputPaths);
      }
    } catch (error) {
      pushLog(`Error: ${String(error)}`, "error");
    } finally {
      isConverting = false;
      progress = null;
    }
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
          <button class="mode-tab" class:active={activeMode === "convert"} onclick={() => (activeMode = "convert")}>
            Convert
          </button>
          <button class="mode-tab" class:active={activeMode === "merge"} onclick={() => (activeMode = "merge")}>
            Merge
          </button>
          <button class="mode-tab" class:active={activeMode === "align"} onclick={() => (activeMode = "align")}>
            Align
          </button>
        </div>
      </div>

      <div class="settings-scroll">
      {#if activeMode === "convert"}
      <label class="field">
        <span>Output format</span>
        <div class="format-groups">
          {#each [...new Set(formats.map((format) => format.category))] as category}
            <div class="format-group">
              <p>{categoryLabels[category] ?? category}</p>
              <div class="pills">
                {#each formats.filter((format) => format.category === category) as format}
                  <button
                    class="pill"
                    class:active={outputFormat === format.id}
                    onclick={() => (outputFormat = format.id)}
                    disabled={isBusy}
                  >
                    {format.label}
                  </button>
                {/each}
              </div>
            </div>
          {/each}
        </div>
      </label>

      <label class="field">
        <span>Output folder</span>
        <div class="row">
          <input bind:value={outputDir} placeholder="C:\path\to\output" disabled={isConverting} />
          <button class="ghost" onclick={browseOutputDir} disabled={isConverting}>Choose</button>
        </div>
      </label>

      <label class="field">
        <span>Filename prefix (optional)</span>
        <input bind:value={prefix} placeholder="run42_" disabled={isConverting} />
      </label>

      <label class="toggle">
        <input type="checkbox" bind:checked={compress} disabled={isConverting || outputFormat === "cram"} />
        <span>Gzip compress text outputs (.gz)</span>
        <HelpTip text="Applies to text formats such as FASTA, FASTQ, SAM, GFF, and VCF. Binary BAM/CRAM are always compressed internally." />
      </label>

      {#if needsReferenceForConvert}
        <label class="field">
          <span class="label-with-help">
            <span>Reference FASTA (required for CRAM)</span>
            <HelpTip text="CRAM stores differences against a reference FASTA. Required when converting to or from CRAM. The app also looks for a matching FASTA next to each CRAM file. Right-click a FASTA and choose Set as reference." />
          </span>
          <div class="row">
            <input
              bind:value={referencePath}
              placeholder="C:\path\to\reference.fasta"
              disabled={isConverting}
            />
            <button class="ghost" onclick={browseReferenceFasta} disabled={isConverting}>Choose</button>
          </div>
          <p class="subtle">
            {#if outputFormat === "cram"}
              Applies to all selected SAM/BAM files ({alignmentInputPaths.length} of {selectedPaths.length} selected).
            {:else if selectedHasCram}
              Used to decode {selectedPaths.filter((path) => /\.cram$/i.test(path)).length} selected CRAM file(s).
            {/if}
          </p>
        </label>
      {/if}

      <button class="primary" onclick={startConversion} disabled={isConverting}>
        {isConverting ? "Converting…" : "Run conversion"}
      </button>

      {#if progress}
        <div class="progress-wrap">
          <div class="progress-bar" style={`width: ${(progress.current / progress.total) * 100}%`}></div>
        </div>
        <p class="progress-label">{progress.current} / {progress.total} — {progress.fileName}</p>
      {/if}

      {#if lastSummary}
        <div class="success">
          <strong>Conversion complete</strong>
          <p>{lastSummary.files.length} files · {lastSummary.totalRecords.toLocaleString()} records</p>
        </div>
      {/if}
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
      radial-gradient(circle at top left, rgba(34, 211, 238, 0.12), transparent 28%),
      radial-gradient(circle at 85% 10%, rgba(16, 185, 129, 0.1), transparent 24%),
      #070b14;
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
    outline: 1px dashed rgba(103, 232, 249, 0.55);
    outline-offset: -1px;
    background: rgba(8, 47, 73, 0.22);
  }

  .panel-hint {
    color: #94a3b8;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .files-panel :global(.explorer) {
    flex: 1 1 auto;
    min-height: 0;
  }

  .resize-handle {
    width: 10px;
    margin: 0 6px;
    border-radius: 999px;
    cursor: col-resize;
    background: linear-gradient(180deg, rgba(148, 163, 184, 0.08), rgba(148, 163, 184, 0.22));
    border: 1px solid rgba(148, 163, 184, 0.12);
    align-self: stretch;
    transition: background 0.15s ease, border-color 0.15s ease;
  }

  .resize-handle:hover,
  .workspace.resizing .resize-handle {
    background: linear-gradient(180deg, rgba(34, 211, 238, 0.18), rgba(16, 185, 129, 0.18));
    border-color: rgba(103, 232, 249, 0.35);
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
    background: rgba(10, 16, 28, 0.82);
    border: 1px solid rgba(148, 163, 184, 0.14);
    border-radius: 20px;
    padding: 18px;
    backdrop-filter: blur(10px);
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.22);
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
    border: 1px solid rgba(148, 163, 184, 0.16);
    background: rgba(30, 41, 59, 0.75);
    color: #cbd5e1;
    padding: 6px 12px;
    border-radius: 999px;
    font-size: 0.82rem;
    font-weight: 600;
  }

  .mode-tab.active {
    color: #ecfeff;
    border-color: rgba(103, 232, 249, 0.45);
    background: linear-gradient(135deg, rgba(34, 211, 238, 0.22), rgba(16, 185, 129, 0.22));
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
    color: #cbd5e1;
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
    color: #94a3b8;
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
    border: 1px solid rgba(148, 163, 184, 0.18);
    background: rgba(15, 23, 42, 0.85);
    color: #e8eef8;
  }

  .format-groups {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .format-group p {
    margin: 0 0 6px;
    color: #94a3b8;
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
    background: rgba(30, 41, 59, 0.9);
    color: #cbd5e1;
    border: 1px solid rgba(148, 163, 184, 0.14);
    transition: all 0.15s ease;
  }

  .pill.active {
    background: linear-gradient(135deg, rgba(34, 211, 238, 0.22), rgba(16, 185, 129, 0.22));
    color: #ecfeff;
    border-color: rgba(103, 232, 249, 0.45);
  }

  .ghost,
  .primary {
    padding: 10px 14px;
    border-radius: 12px;
    font-weight: 600;
  }

  .ghost {
    background: rgba(30, 41, 59, 0.9);
    color: #e2e8f0;
    border: 1px solid rgba(148, 163, 184, 0.16);
  }

  .primary {
    width: 100%;
    margin-top: 8px;
    background: linear-gradient(135deg, #0891b2, #059669);
    color: white;
    box-shadow: 0 10px 30px rgba(8, 145, 178, 0.25);
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
    border: 1px solid rgba(148, 163, 184, 0.16);
    background: rgba(30, 41, 59, 0.75);
    color: #cbd5e1;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 0.78rem;
    font-weight: 600;
  }

  .log-tab.active {
    color: #ecfeff;
    border-color: rgba(103, 232, 249, 0.45);
    background: linear-gradient(135deg, rgba(34, 211, 238, 0.22), rgba(16, 185, 129, 0.22));
  }

  .progress-wrap {
    margin-top: 14px;
    height: 8px;
    border-radius: 999px;
    background: rgba(30, 41, 59, 0.95);
    overflow: hidden;
  }

  .progress-bar {
    height: 100%;
    border-radius: inherit;
    background: linear-gradient(90deg, #22d3ee, #34d399);
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
    background: rgba(6, 78, 59, 0.28);
    border: 1px solid rgba(52, 211, 153, 0.25);
  }

  .log {
    margin: 0;
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: 14px;
    border-radius: 14px;
    background: rgba(2, 6, 23, 0.72);
    border: 1px solid rgba(148, 163, 184, 0.1);
    color: #a5b4fc;
    font-family: "JetBrains Mono", monospace;
    font-size: 0.78rem;
    line-height: 1.5;
  }

  .log-line {
    white-space: pre-wrap;
    margin-bottom: 2px;
  }

  .log-line.log-error {
    color: #f87171;
  }

  .log-line.log-warn {
    color: #fb923c;
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