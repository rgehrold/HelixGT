<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import HelpTip from "$lib/components/HelpTip.svelte";
  import {
    cancelJob,
    onConvertProgress,
    runConversion,
    runFastqQc,
    runPreflightCheck,
    suggestOutputFormat,
  } from "$lib/api";
  import type {
    ConvertProgress,
    ConvertSummary,
    FormatInfo,
    LogLevel,
    PreflightResponse,
  } from "$lib/types";

  interface Props {
    formats: FormatInfo[];
    selectedPaths: string[];
    outputDir: string;
    outputFormat: string;
    compress: boolean;
    prefix: string;
    referencePath: string;
    disabled?: boolean;
    samtoolsAvailable?: boolean;
    onOutputFormatChange: (value: string) => void;
    onOutputDirChange: (value: string) => void;
    onLog: (message: string, level?: LogLevel) => void;
    onComplete: (summary: ConvertSummary) => Promise<void>;
    onBusyChange?: (busy: boolean) => void;
  }

  let {
    formats,
    selectedPaths,
    outputDir,
    outputFormat,
    compress = $bindable(),
    prefix = $bindable(),
    referencePath = $bindable(),
    disabled = false,
    samtoolsAvailable = false,
    onOutputFormatChange,
    onOutputDirChange,
    onLog,
    onComplete,
    onBusyChange,
  }: Props = $props();

  const categoryOrder = ["sequence", "alignment", "annotation", "variants"] as const;

  const categoryLabels: Record<string, string> = {
    sequence: "Sequence",
    alignment: "Alignment",
    annotation: "Annotation",
    variants: "Variants",
  };

  let isConverting = $state(false);
  let isPreflighting = $state(false);
  let isRunningQc = $state(false);
  let progress = $state<ConvertProgress | null>(null);
  let lastSummary = $state<ConvertSummary | null>(null);
  let preflight = $state<PreflightResponse | null>(null);
  let currentJobId = $state<string | null>(null);

  const selectedHasCram = $derived(selectedPaths.some((path) => /\.cram$/i.test(path)));

  const needsReference = $derived(outputFormat === "cram" || selectedHasCram);

  const alignmentInputPaths = $derived(
    selectedPaths.filter((path) => /\.(sam|bam|cram)(\.gz)?$/i.test(path)),
  );

  const conversionInputPaths = $derived(
    outputFormat === "cram" ? alignmentInputPaths : selectedPaths,
  );

  const singleFastqPath = $derived.by(() => {
    if (selectedPaths.length !== 1) return null;
    const path = selectedPaths[0];
    return /\.(fastq|fq)(\.gz)?$/i.test(path) ? path : null;
  });

  $effect(() => {
    void refreshSuggestedFormat(selectedPaths);
  });

  onMount(() => {
    let unlisten: UnlistenFn | undefined;

    void onConvertProgress((payload) => {
      progress = payload;
      onLog(`[${payload.current}/${payload.total}] ${payload.fileName}`);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  });

  async function refreshSuggestedFormat(paths: string[]) {
    preflight = null;
    if (paths.length === 0) return;

    try {
      const suggested = await suggestOutputFormat(paths);
      if (suggested && formats.some((format) => format.id === suggested)) {
        onOutputFormatChange(suggested);
      }
    } catch (error) {
      onLog(`Could not suggest output format: ${String(error)}`, "warn");
    }
  }

  async function browseOutputDir() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose output folder",
    });
    if (picked) onOutputDirChange(String(picked));
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

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  async function runPreflight() {
    isPreflighting = true;
    preflight = null;
    onLog("Running preflight check…");

    try {
      const report = await runPreflightCheck({
        mode: "convert",
        inputPaths: conversionInputPaths,
        outputDir: outputDir.trim(),
        outputFormat,
        referencePath: needsReference ? referencePath.trim() || null : null,
      });
      preflight = report;

      for (const issue of report.issues) {
        onLog(issue.message, issue.severity === "error" ? "error" : "warn");
      }

      if (report.ok) {
        onLog(
          `Preflight passed — estimated output ${formatBytes(report.estimatedOutputBytes)}.`,
        );
      } else {
        onLog("Preflight found blocking issues.", "error");
      }
    } catch (error) {
      onLog(`Preflight error: ${String(error)}`, "error");
    } finally {
      isPreflighting = false;
    }
  }

  async function runQc() {
    if (!singleFastqPath) return;

    isRunningQc = true;
    onLog(`Running FASTQ QC on ${singleFastqPath}…`);

    try {
      const summary = await runFastqQc(singleFastqPath);
      onLog(
        `FASTQ QC — ${summary.readCount.toLocaleString()} reads, ${summary.totalBases.toLocaleString()} bases, mean length ${summary.meanReadLength.toFixed(1)}, N fraction ${(summary.nContentFraction * 100).toFixed(2)}%.`,
      );
    } catch (error) {
      onLog(`FASTQ QC error: ${String(error)}`, "error");
    } finally {
      isRunningQc = false;
    }
  }

  function validateBeforeRun(): string[] {
    const errors: string[] = [];

    if (selectedPaths.length === 0) {
      errors.push("Select at least one compatible input file in the browser.");
    }
    if (!outputDir.trim()) {
      errors.push("Choose an output folder.");
    }
    if (needsReference) {
      if (!referencePath.trim()) {
        errors.push("Choose a reference FASTA for CRAM conversion or decoding.");
      }
      if (!samtoolsAvailable) {
        errors.push("CRAM conversion requires samtools on PATH or in src-tauri/binaries/.");
      }
    }
    if (outputFormat === "cram") {
      if (alignmentInputPaths.length === 0) {
        errors.push("Select at least one SAM or BAM file to convert to CRAM.");
      }
    } else if (conversionInputPaths.length === 0) {
      errors.push("Select at least one compatible input file in the browser.");
    }

    return errors;
  }

  async function startConversion() {
    lastSummary = null;
    preflight = null;

    const errors = validateBeforeRun();
    if (errors.length > 0) {
      for (const message of errors) {
        onLog(message, "error");
      }
      return;
    }

    const inputPaths = conversionInputPaths;

    if (outputFormat === "cram" && inputPaths.length < selectedPaths.length) {
      onLog(
        `Converting ${inputPaths.length} alignment file(s); skipped ${selectedPaths.length - inputPaths.length} non-alignment file(s).`,
        "warn",
      );
    }

    isPreflighting = true;
    try {
      const report = await runPreflightCheck({
        mode: "convert",
        inputPaths,
        outputDir: outputDir.trim(),
        outputFormat,
        referencePath: needsReference ? referencePath.trim() || null : null,
      });
      preflight = report;

      if (!report.ok) {
        for (const issue of report.issues) {
          onLog(issue.message, issue.severity === "error" ? "error" : "warn");
        }
        onLog("Preflight failed — fix issues before converting.", "error");
        return;
      }
    } catch (error) {
      onLog(`Preflight error: ${String(error)}`, "error");
      return;
    } finally {
      isPreflighting = false;
    }

    const jobId = `convert-${Date.now()}`;
    currentJobId = jobId;
    isConverting = true;
    progress = null;
    onBusyChange?.(true);
    onLog("Starting batch conversion…");

    try {
      const summary = await runConversion({
        inputPaths,
        outputDir: outputDir.trim(),
        outputFormat,
        compress,
        prefix: prefix.trim() || null,
        referencePath: needsReference ? referencePath.trim() : null,
        jobId,
      });
      lastSummary = summary;
      onLog(
        `Done — ${summary.files.length} file(s), ${summary.totalRecords.toLocaleString()} records.`,
      );
      if (summary.partialFailure) {
        onLog(summary.errorMessage ?? "Some files failed to convert.", "warn");
      }
      await onComplete(summary);
    } catch (error) {
      const message = String(error);
      if (/cancel/i.test(message)) {
        onLog("Conversion cancelled.", "warn");
      } else {
        onLog(`Error: ${message}`, "error");
      }
    } finally {
      isConverting = false;
      currentJobId = null;
      progress = null;
      onBusyChange?.(false);
    }
  }

  async function cancelConversion() {
    if (!currentJobId) return;
    const cancelled = await cancelJob(currentJobId);
    if (cancelled) {
      onLog("Cancellation requested…", "warn");
    } else {
      onLog("Could not cancel conversion.", "warn");
    }
  }

  async function openOutputFolder() {
    if (!lastSummary?.files.length) return;

    try {
      await revealItemInDir(lastSummary.files[0].outputPath);
    } catch (error) {
      if (outputDir.trim()) {
        try {
          await openPath(outputDir.trim());
        } catch (fallbackError) {
          onLog(`Could not open output folder: ${String(fallbackError)}`, "error");
        }
      } else {
        onLog(`Could not open output folder: ${String(error)}`, "error");
      }
    }
  }

  const isBusy = $derived(isConverting || isPreflighting || isRunningQc);
</script>

<div class="pane-body">
  <p class="intro">
    Convert selected files to another genomics format. Output format is suggested automatically from
    your selection.
  </p>

  <label class="field">
    <span>Output format</span>
    <div class="format-groups">
      {#each categoryOrder as category}
        {@const categoryFormats = formats.filter((format) => format.category === category)}
        {#if categoryFormats.length > 0}
          <div class="format-group">
            <p>{categoryLabels[category] ?? category}</p>
            <div class="pills">
              {#each categoryFormats as format}
                <button
                  class="pill"
                  class:active={outputFormat === format.id}
                  onclick={() => onOutputFormatChange(format.id)}
                  disabled={disabled || isBusy}
                >
                  {format.label}
                </button>
              {/each}
            </div>
          </div>
        {/if}
      {/each}
    </div>
  </label>

  <label class="field">
    <span>Output folder</span>
    <div class="row">
      <input
        value={outputDir}
        oninput={(event) => onOutputDirChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="C:\path\to\output"
        disabled={disabled || isBusy}
      />
      <button class="ghost" onclick={browseOutputDir} disabled={disabled || isBusy}>Choose</button>
    </div>
  </label>

  <label class="field">
    <span>Filename prefix (optional)</span>
    <input bind:value={prefix} placeholder="run42_" disabled={disabled || isBusy} />
  </label>

  <label class="toggle">
    <input
      type="checkbox"
      checked={compress}
      onchange={(event) => (compress = (event.currentTarget as HTMLInputElement).checked)}
      disabled={disabled || isBusy || outputFormat === "cram"}
    />
    <span>Gzip compress text outputs (.gz)</span>
    <HelpTip text="Applies to text formats such as FASTA, FASTQ, SAM, GFF, and VCF. Binary BAM/CRAM are always compressed internally." />
  </label>

  {#if needsReference}
    <label class="field">
      <span class="label-with-help">
        <span>Reference FASTA (required for CRAM)</span>
        <HelpTip text="CRAM stores differences against a reference FASTA. Required when converting to or from CRAM. The app also looks for a matching FASTA next to each CRAM file. Right-click a FASTA and choose Set as reference." />
      </span>
      <div class="row">
        <input
          bind:value={referencePath}
          placeholder="C:\path\to\reference.fasta"
          disabled={disabled || isBusy}
        />
        <button class="ghost" onclick={browseReferenceFasta} disabled={disabled || isBusy}>
          Choose
        </button>
      </div>
      <p class="subtle">
        {#if outputFormat === "cram"}
          Applies to all selected SAM/BAM files ({alignmentInputPaths.length} of {selectedPaths.length}
          selected).
        {:else if selectedHasCram}
          Used to decode {selectedPaths.filter((path) => /\.cram$/i.test(path)).length} selected CRAM
          file(s).
        {/if}
      </p>
    </label>
  {/if}

  {#if preflight}
    <div class="status" class:ok={preflight.ok} class:bad={!preflight.ok}>
      {#if preflight.ok}
        Preflight passed — estimated output {formatBytes(preflight.estimatedOutputBytes)}.
      {:else}
        Preflight found {preflight.issues.length} issue(s).
      {/if}
    </div>
  {/if}

  <div class="actions">
    <button class="ghost" onclick={runPreflight} disabled={disabled || isBusy}>
      {isPreflighting ? "Checking…" : "Preflight check"}
    </button>
    {#if singleFastqPath}
      <button class="ghost" onclick={runQc} disabled={disabled || isBusy}>
        {isRunningQc ? "Running QC…" : "FASTQ QC"}
      </button>
    {/if}
  </div>

  {#if isConverting}
    <div class="actions">
      <button class="primary" disabled>Converting…</button>
      <button class="ghost danger" onclick={cancelConversion}>Cancel</button>
    </div>
  {:else}
    <button class="primary" onclick={startConversion} disabled={disabled || isBusy}>
      Run conversion
    </button>
  {/if}

  {#if progress}
    <div class="progress-wrap">
      <div
        class="progress-bar"
        style={`width: ${(progress.current / progress.total) * 100}%`}
      ></div>
    </div>
    <p class="progress-label">{progress.current} / {progress.total} — {progress.fileName}</p>
  {/if}

  {#if lastSummary?.partialFailure}
    <div class="warning">
      <strong>Partial failure</strong>
      {#if lastSummary.errorMessage}
        <p>{lastSummary.errorMessage}</p>
      {:else}
        <p>Some input files could not be converted.</p>
      {/if}
    </div>
  {/if}

  {#if lastSummary}
    <div class="success">
      <strong>Conversion complete</strong>
      <p>{lastSummary.files.length} files · {lastSummary.totalRecords.toLocaleString()} records</p>
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

  .ghost.danger {
    color: var(--status-error-text);
    border-color: var(--status-error-border);
    background: var(--status-error-bg);
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

  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }

  .actions .primary {
    flex: 1 1 auto;
    margin-top: 0;
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

  .progress-label {
    margin: 8px 0 0;
    font-size: 0.9rem;
  }

  .warning {
    margin-top: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--status-error-bg);
    border: 1px solid var(--status-error-border);
    color: var(--status-error-text);
  }

  .warning p {
    margin: 8px 0 0;
    font-size: 0.88rem;
    white-space: pre-wrap;
  }

  .success {
    margin-top: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--success-bg);
    border: 1px solid var(--success-border);
  }

  .success p {
    margin: 8px 0 10px;
    font-size: 0.9rem;
  }

  .success .ghost {
    width: 100%;
  }
</style>