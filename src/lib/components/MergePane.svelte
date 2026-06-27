<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import type { LogLevel, MergeSuggestion, MergeSummary, MergeValidation } from "$lib/types";

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

    validation = await invoke<MergeValidation>("validate_merge_paths", { paths });
    suggestion = await invoke<MergeSuggestion>("suggest_merged_filename", { paths });
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

    isMerging = true;
    onBusyChange?.(true);
    onLog(`Merging ${selectedPaths.length} file(s)…`);

    try {
      const summary = await invoke<MergeSummary>("run_merge", {
        request: {
          inputPaths: selectedPaths,
          outputDir: outputDir.trim(),
          outputName: outputName.trim(),
          compress,
        },
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
      onBusyChange?.(false);
    }
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

  <button class="primary" onclick={runMerge} disabled={disabled || isMerging || !validation?.isValid}>
    {isMerging ? "Merging…" : "Run merge"}
  </button>

  {#if lastSummary}
    <div class="success">
      <strong>Merge complete</strong>
      <p>{lastSummary.inputCount} inputs · {lastSummary.records.toLocaleString()} records</p>
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