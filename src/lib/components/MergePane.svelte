<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
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
  let referencePath = $state("");

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
      referencePath: validation?.format === "cram" ? referencePath.trim() || null : null,
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
        referencePath:
          validation?.format === "cram" ? referencePath.trim() || null : null,
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
  <p class="intro">
    Combine multiple files with the <strong>same extension</strong>. Similar filenames are detected
    automatically and a merged name is suggested.
  </p>

  {#if validation}
    <div class="status" class:ok={validation.isValid} class:bad={!validation.isValid}>
      {validation.message}
    </div>
  {/if}

  {#if suggestion?.similar}
    <div class="hint-box">
      <span>Similar names detected</span>
      {#if suggestion.commonPrefix}
        <p>Common prefix: <code>{suggestion.commonPrefix}</code></p>
      {/if}
      {#if suggestion.commonSuffix}
        <p>Common suffix: <code>{suggestion.commonSuffix}</code></p>
      {/if}
    </div>
  {/if}

  <label class="field">
    <span>Output folder</span>
    <div class="row">
      <input
        value={outputDir}
        oninput={(event) => onOutputDirChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="C:\path\to\output"
        disabled={disabled || isMerging}
      />
      <button class="ghost" onclick={browseOutputDir} disabled={disabled || isMerging}>Choose</button>
    </div>
  </label>

  <label class="field">
    <span>Merged filename</span>
    <input bind:value={outputName} placeholder="sample_merged.fasta.gz" disabled={disabled || isMerging} />
  </label>

  <label class="toggle">
    <input
      type="checkbox"
      checked={compress ?? false}
      onchange={(event) => (compress = (event.currentTarget as HTMLInputElement).checked)}
      disabled={disabled || isMerging}
    />
    <span>Gzip compress output (.gz)</span>
  </label>

  {#if validation?.format === "cram"}
    <label class="field">
      <span>Reference FASTA (required for CRAM merge)</span>
      <input bind:value={referencePath} placeholder="C:\path\to\reference.fasta" disabled={disabled || isMerging} />
    </label>
  {/if}

  <div class="row actions">
    <button class="primary" onclick={runMerge} disabled={disabled || isMerging || !validation?.isValid}>
      {isMerging ? "Merging…" : "Run merge"}
    </button>
    {#if isMerging}
      <button class="ghost" onclick={cancelMerge}>Cancel</button>
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

  .hint-box {
    margin-bottom: 14px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--chip-active-border);
    background: var(--tree-focus);
    color: var(--code-color);
    font-size: 0.84rem;
  }

  .hint-box span {
    display: block;
    font-weight: 600;
    margin-bottom: 6px;
  }

  .hint-box p {
    margin: 4px 0 0;
  }

  code {
    font-family: "JetBrains Mono", monospace;
    font-size: 0.8rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .field > span,
  .toggle span {
    color: var(--text-menu);
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
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
  }

  .ghost,
  .primary {
    cursor: pointer;
    border: none;
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
  .ghost:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
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