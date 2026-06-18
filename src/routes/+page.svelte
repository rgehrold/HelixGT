<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import FileExplorer from "$lib/components/FileExplorer.svelte";
  import MenuBar from "$lib/components/MenuBar.svelte";
  import type { ConvertProgress, ConvertSummary, FormatInfo } from "$lib/types";

  let formats = $state<FormatInfo[]>([]);
  let selectedPaths = $state<string[]>([]);
  let outputDir = $state("");
  let outputFormat = $state("fasta");
  let compress = $state(true);
  let prefix = $state("");
  let isConverting = $state(false);
  let isDragging = $state(false);
  let progress = $state<ConvertProgress | null>(null);
  let logs = $state<string[]>([]);
  let errorMessage = $state("");
  let lastSummary = $state<ConvertSummary | null>(null);
  let fileExplorer = $state<{
    revealPaths: (paths: string[]) => Promise<void>;
    refreshTree: () => Promise<void>;
  } | null>(null);
  let filesPaneWidth = $state(58);
  let isResizing = $state(false);
  let logEl = $state<HTMLPreElement | null>(null);

  const categoryLabels: Record<string, string> = {
    sequence: "Sequence",
    alignment: "Alignment",
    annotation: "Annotation",
    variants: "Variants",
  };

  $effect(() => {
    logs;
    if (logEl) {
      logEl.scrollTop = logEl.scrollHeight;
    }
  });

  onMount(async () => {
    pushLog("Ready.");
    formats = await invoke<FormatInfo[]>("get_supported_formats");
    await listen<ConvertProgress>("convert-progress", (event) => {
      progress = event.payload;
      logs = [
        ...logs,
        `[${event.payload.current}/${event.payload.total}] ${event.payload.fileName}`,
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

  function pushLog(message: string) {
    logs = [...logs, message];
  }

  async function addPaths(paths: string[]) {
    if (paths.length === 0) return;
    const valid = await invoke<string[]>("validate_input_paths", { paths });
    const merged = new Set([...selectedPaths, ...valid]);
    selectedPaths = [...merged].sort();
    if (valid.length < paths.length) {
      pushLog(`Skipped ${paths.length - valid.length} unsupported file(s).`);
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

  function onDragOver(event: DragEvent) {
    event.preventDefault();
  }

  async function startConversion() {
    errorMessage = "";
    lastSummary = null;

    if (selectedPaths.length === 0) {
      errorMessage = "Select at least one compatible input file in the browser.";
      return;
    }
    if (!outputDir.trim()) {
      errorMessage = "Choose an output folder.";
      return;
    }

    isConverting = true;
    progress = null;
    pushLog("Starting batch conversion…");

    try {
      const summary = await invoke<ConvertSummary>("run_conversion", {
        request: {
          inputPaths: selectedPaths,
          outputDir: outputDir.trim(),
          outputFormat,
          compress,
          prefix: prefix.trim() || null,
        },
      });
      lastSummary = summary;
      pushLog(
        `Done — ${summary.files.length} file(s), ${summary.totalRecords.toLocaleString()} records.`,
      );
      await fileExplorer?.refreshTree();
      const outputPaths = summary.files.map((file) => file.outputPath);
      if (outputPaths.length > 0) {
        await fileExplorer?.revealPaths(outputPaths);
      }
    } catch (error) {
      errorMessage = String(error);
      pushLog(`Error: ${errorMessage}`);
    } finally {
      isConverting = false;
      progress = null;
    }
  }
</script>

<div class="app">
  <MenuBar />

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
        disabled={isConverting}
        onSetOutputFolder={(path) => {
          outputDir = path;
          pushLog(`Output folder set to ${path}`);
        }}
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
        <h2>Conversion</h2>
      </div>

      <div class="settings-scroll">
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
                    disabled={isConverting}
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
        <input type="checkbox" bind:checked={compress} disabled={isConverting} />
        <span>Gzip compress text outputs (.gz)</span>
      </label>

      <button class="primary" onclick={startConversion} disabled={isConverting}>
        {isConverting ? "Converting…" : "Run conversion"}
      </button>

      {#if progress}
        <div class="progress-wrap">
          <div class="progress-bar" style={`width: ${(progress.current / progress.total) * 100}%`}></div>
        </div>
        <p class="progress-label">{progress.current} / {progress.total} — {progress.fileName}</p>
      {/if}

      {#if errorMessage}
        <p class="error">{errorMessage}</p>
      {/if}

      {#if lastSummary}
        <div class="success">
          <strong>Conversion complete</strong>
          <p>{lastSummary.files.length} files · {lastSummary.totalRecords.toLocaleString()} records</p>
        </div>
      {/if}
      </div>
    </section>

    <section class="panel log-panel">
      <div class="panel-head">
        <h2>Activity</h2>
      </div>
      <pre class="log" bind:this={logEl}>{logs.join("\n")}</pre>
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

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .field > span,
  .toggle span {
    color: #cbd5e1;
    font-size: 0.92rem;
    font-weight: 500;
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
  .error,
  .success p {
    margin: 8px 0 0;
    font-size: 0.9rem;
  }

  .error {
    color: #fca5a5;
    white-space: pre-wrap;
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
    white-space: pre-wrap;
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