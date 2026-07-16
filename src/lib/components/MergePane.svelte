<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import InfoLink from "$lib/components/InfoLink.svelte";
  import {
    cancelJob,
    onMergeProgress,
    runMerge as invokeMerge,
    runPreflightCheck,
    suggestMergedFilename,
    validateMergePaths,
  } from "$lib/api";
  import type { LogLevel, MergeProgress, MergeSuggestion, MergeSummary, MergeValidation } from "$lib/types";

  interface Props {
    selectedPaths: string[];
    outputDir: string;
    disabled?: boolean;
    onOutputDirChange: (value: string) => void;
    onLog: (message: string, level?: LogLevel) => void;
    onComplete: (outputPath: string) => Promise<void>;
    onBusyChange?: (busy: boolean) => void;
  }

  let {
    selectedPaths,
    outputDir,
    disabled = false,
    onOutputDirChange,
    onLog,
    onComplete,
    onBusyChange,
  }: Props = $props();

  let outputName = $state("");
  let compress = $state<boolean | null>(null);
  let validation = $state<MergeValidation | null>(null);
  let suggestion = $state<MergeSuggestion | null>(null);
  let isMerging = $state(false);
  let lastSummary = $state<MergeSummary | null>(null);
  let mergeProgress = $state<MergeProgress | null>(null);
  let currentJobId = $state<string | null>(null);

  onMount(() => {
    let unlisten: (() => void) | undefined;
    void onMergeProgress((payload) => {
      mergeProgress = payload;
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  });

  $effect(() => {
    void refreshMergeState(selectedPaths);
  });

  async function refreshMergeState(paths: string[]) {
    lastSummary = null;
    if (paths.length === 0) {
      validation = null;
      suggestion = null;
      outputName = "";
      return;
    }

    validation = await validateMergePaths(paths);
    suggestion = await suggestMergedFilename(paths);
    if (!outputName.trim() || suggestion.similar) {
      outputName = suggestion.suggestedName;
    }
    if (compress === null) {
      compress = paths.every((path) => path.toLowerCase().endsWith(".gz"));
    }
  }

  async function browseOutputDir() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose merge output folder",
    });
    if (picked) onOutputDirChange(String(picked));
  }

  async function runMerge() {
    lastSummary = null;

    if (!validation?.isValid) {
      onLog(validation?.message ?? "Select at least two files with the same extension.", "error");
      return;
    }
    if (!outputDir.trim()) {
      onLog("Choose an output folder.", "error");
      return;
    }
    if (!outputName.trim()) {
      onLog("Enter an output filename.", "error");
      return;
    }

    const preflight = await runPreflightCheck({
      mode: "merge",
      inputPaths: selectedPaths,
      outputDir: outputDir.trim(),
    });
    if (!preflight.ok) {
      onLog(preflight.issues.map((issue) => issue.message).join("\n"), "error");
      return;
    }

    isMerging = true;
    mergeProgress = null;
    onBusyChange?.(true);
    currentJobId = `merge-${Date.now()}`;
    onLog(`Merging ${selectedPaths.length} file(s)…`);

    try {
      const summary = await invokeMerge({
        inputPaths: selectedPaths,
        outputDir: outputDir.trim(),
        outputName: outputName.trim(),
        compress,
        jobId: currentJobId,
      });
      lastSummary = summary;
      onLog(
        `Merge complete — ${summary.records.toLocaleString()} records written to ${summary.outputPath}`,
      );
      await onComplete(summary.outputPath);
    } catch (error) {
      onLog(`Merge error: ${String(error)}`, "error");
    } finally {
      isMerging = false;
      mergeProgress = null;
      currentJobId = null;
      onBusyChange?.(false);
    }
  }

  async function cancelMerge() {
    if (!currentJobId) return;
    await cancelJob(currentJobId);
    onLog("Merge cancelled.", "warn");
  }

  async function openOutputFolder() {
    if (!lastSummary?.outputPath) return;
    const slash = Math.max(
      lastSummary.outputPath.lastIndexOf("\\"),
      lastSummary.outputPath.lastIndexOf("/"),
    );
    if (slash > 0) await openPath(lastSummary.outputPath.slice(0, slash));
  }
</script>

<div class="pane-body">
  {#if validation}
    <div class="status compact" class:ok={validation.isValid} class:bad={!validation.isValid}>
      {validation.message}
    </div>
  {/if}

  {#if suggestion?.similar}
    <p class="subtle hint-line">
      Similar names
      {#if suggestion.commonPrefix} · prefix <code>{suggestion.commonPrefix}</code>{/if}
      {#if suggestion.commonSuffix} · suffix <code>{suggestion.commonSuffix}</code>{/if}
    </p>
  {/if}

  <div class="field">
    <span class="field-label">Output folder <InfoLink section="merge-output-folder" label="Output folder — manual" /></span>
    <div class="row">
      <button class="ghost compact" onclick={browseOutputDir} disabled={disabled || isMerging}>Choose</button>
      <input
        value={outputDir}
        oninput={(event) => onOutputDirChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="Output folder"
        disabled={disabled || isMerging}
      />
    </div>
  </div>

  <div class="field">
    <span class="field-label">Merged filename <InfoLink section="merge-filename" label="Merged filename — manual" /></span>
    <input bind:value={outputName} placeholder="sample_merged.fasta.gz" disabled={disabled || isMerging} />
  </div>

  <label class="toggle inline">
    <input
      type="checkbox"
      checked={compress ?? false}
      onchange={(event) => (compress = (event.currentTarget as HTMLInputElement).checked)}
      disabled={disabled || isMerging}
    />
    <span>Gzip output</span>
    <InfoLink section="merge-gzip" label="Gzip compress — manual" />
  </label>

  {#if validation?.format === "cram"}
    <p class="subtle">CRAM concat uses first header — convert to BAM for re-encode.</p>
  {/if}

  <div class="row actions">
    <button class="primary" onclick={runMerge} disabled={disabled || isMerging || !validation?.isValid}>
      {isMerging ? "Merging…" : "Run merge"}
    </button>
    {#if isMerging}
      <button class="ghost compact" onclick={cancelMerge}>Cancel</button>
    {/if}
  </div>

  {#if mergeProgress}
    <p class="progress-label">{mergeProgress.message}</p>
  {/if}

  {#if lastSummary}
    <div class="success">
      <strong>Merge complete</strong>
      <p>{lastSummary.inputCount} inputs · {lastSummary.records.toLocaleString()} records</p>
      <button class="ghost" onclick={openOutputFolder}>Open output folder</button>
    </div>
  {/if}
</div>

<style>
  .pane-body {
    display: flex;
    flex-direction: column;
    gap: 0;
  }

  .intro {
    margin: 0 0 14px;
    color: var(--text-muted);
    font-size: 0.88rem;
    line-height: 1.5;
  }

  .status {
    margin-bottom: 14px;
    padding: 10px 12px;
    border-radius: 12px;
    font-size: 0.88rem;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
  }

  .status.ok {
    border-color: var(--success-border);
    background: var(--success-bg);
    color: var(--accent-highlight);
  }

  .status.bad {
    border-color: var(--status-error-border);
    background: var(--status-error-bg);
    color: var(--status-error-text);
  }

  .hint-line {
    margin: 0 0 8px;
  }

  code {
    font-family: "JetBrains Mono", monospace;
    font-size: 0.76rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }

  .field-label {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-menu);
    font-size: 0.78rem;
    font-weight: 600;
  }

  .row {
    display: flex;
    align-items: stretch;
    gap: 6px;
    min-width: 0;
  }

  .row > .ghost {
    flex: 0 0 auto;
  }

  .row > input:not([type="checkbox"]) {
    flex: 1 1 auto;
    min-width: 0;
    width: auto;
  }

  input:not([type="checkbox"]) {
    width: 100%;
    padding: 7px 10px;
    border-radius: 8px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    box-sizing: border-box;
    font-size: 0.84rem;
  }

  input:not([type="checkbox"]):focus,
  input:not([type="checkbox"]):focus-visible {
    outline: none;
    border-color: var(--accent-highlight);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-highlight) 55%, transparent);
    position: relative;
    z-index: 1;
  }

  .ghost,
  .primary {
    cursor: pointer;
    border: none;
    padding: 7px 12px;
    border-radius: 8px;
    font-weight: 600;
    font-size: 0.82rem;
  }

  .ghost.compact {
    padding: 5px 10px;
    font-size: 0.78rem;
  }

  .ghost {
    background: var(--chip-bg);
    color: var(--text-primary);
    border: 1px solid var(--chip-border);
  }

  .primary {
    flex: 1 1 auto;
    margin-top: 0;
    background: var(--primary-bg);
    color: var(--primary-text);
    box-shadow: var(--primary-shadow);
  }

  .toggle.inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 10px;
    font-size: 0.8rem;
    color: var(--text-menu);
    cursor: pointer;
  }

  .status.compact {
    margin-bottom: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    font-size: 0.8rem;
  }

  .subtle {
    margin: 0 0 8px;
    color: var(--text-muted);
    font-size: 0.74rem;
  }

  .actions {
    gap: 6px;
  }

  .primary:disabled,
  .ghost:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
  }

  .toggle-control {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
  }

  .subtle {
    margin: 0 0 12px;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .success {
    margin-top: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--success-bg);
    border: 1px solid var(--success-border);
  }

  .success p {
    margin: 8px 0 0;
    font-size: 0.9rem;
  }
</style>