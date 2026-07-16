<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import InfoLink from "$lib/components/InfoLink.svelte";
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
  /** Once the user picks a format pill, stop overwriting it with auto-suggestions. */
  let userChoseFormat = $state(false);
  let previousSelectionLen = $state(0);

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
    const len = selectedPaths.length;
    // Suggest only when selection first becomes non-empty (or after clear), and only if
    // the user has not manually chosen a format. Avoids stomping their choice every click.
    if (len === 0) {
      previousSelectionLen = 0;
      preflight = null;
      return;
    }
    if (previousSelectionLen === 0 && !userChoseFormat) {
      void refreshSuggestedFormat(selectedPaths);
    } else {
      preflight = null;
    }
    previousSelectionLen = len;
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
    if (paths.length === 0 || userChoseFormat) return;

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
  <div class="field">
    <span class="field-label">Output format <InfoLink section="convert" label="Convert — manual" /></span>
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
                  onclick={() => {
                    userChoseFormat = true;
                    onOutputFormatChange(format.id);
                  }}
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
  </div>

  <div class="field">
    <span class="field-label">Output folder <InfoLink section="convert-output-folder" label="Output folder — manual" /></span>
    <div class="row">
      <button class="ghost compact" onclick={browseOutputDir} disabled={disabled || isBusy}>Choose</button>
      <input
        value={outputDir}
        oninput={(event) => onOutputDirChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="Output folder"
        disabled={disabled || isBusy}
      />
    </div>
  </div>

  <div class="field-row">
    <div class="field grow">
      <span class="field-label">Prefix <InfoLink section="convert-prefix" label="Filename prefix — manual" /></span>
      <input bind:value={prefix} placeholder="optional" disabled={disabled || isBusy} />
    </div>
    <label class="toggle inline">
      <input
        type="checkbox"
        checked={compress}
        onchange={(event) => (compress = (event.currentTarget as HTMLInputElement).checked)}
        disabled={disabled || isBusy || outputFormat === "cram"}
      />
      <span>Gzip text</span>
      <InfoLink section="convert-gzip" label="Gzip compress — manual" />
    </label>
  </div>

  {#if needsReference}
    <div class="field">
      <span class="field-label">CRAM reference <InfoLink section="convert-cram-reference" label="CRAM reference — manual" /></span>
      <div class="row">
        <button class="ghost compact" onclick={browseReferenceFasta} disabled={disabled || isBusy}>Choose</button>
        <input
          bind:value={referencePath}
          placeholder="Reference FASTA"
          disabled={disabled || isBusy}
        />
      </div>
    </div>
  {/if}

  {#if preflight}
    <div class="status compact" class:ok={preflight.ok} class:bad={!preflight.ok}>
      {#if preflight.ok}
        Preflight OK · ~{formatBytes(preflight.estimatedOutputBytes)}
      {:else}
        Preflight: {preflight.issues.length} issue(s)
      {/if}
    </div>
  {/if}

  <div class="actions">
    {#if isConverting}
      <button class="primary" disabled>Converting…</button>
      <button class="ghost compact danger" onclick={cancelConversion}>Cancel</button>
    {:else}
      <button class="primary" onclick={startConversion} disabled={disabled || isBusy}>
        Run conversion
      </button>
    {/if}
    <button class="ghost compact" onclick={runPreflight} disabled={disabled || isBusy} title="Preflight check">
      {isPreflighting ? "…" : "Preflight"}
    </button>
    {#if singleFastqPath}
      <button class="ghost compact" onclick={runQc} disabled={disabled || isBusy} title="FASTQ QC (or use Analyze)">
        {isRunningQc ? "…" : "QC"}
      </button>
    {/if}
  </div>

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

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }

  .field.grow {
    flex: 1;
    margin-bottom: 0;
  }

  .field-row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: flex-end;
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

  .subtle {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.74rem;
    line-height: 1.35;
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

  .format-groups {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .format-group p {
    margin: 0 0 4px;
    color: var(--text-muted);
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .pill,
  .ghost,
  .primary {
    cursor: pointer;
    border: none;
  }

  .pill {
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--chip-bg);
    color: var(--text-menu);
    border: 1px solid var(--chip-border);
    transition: all 0.15s ease;
    font-size: 0.78rem;
  }

  .pill.active {
    background: var(--chip-active-bg);
    color: var(--chip-active-text);
    border-color: var(--chip-active-border);
  }

  .ghost,
  .primary {
    padding: 7px 12px;
    border-radius: 8px;
    font-weight: 600;
    font-size: 0.82rem;
    cursor: pointer;
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

  .ghost.danger {
    color: var(--status-error-text);
    border-color: var(--status-error-border);
    background: var(--status-error-bg);
  }

  .primary {
    flex: 1 1 auto;
    margin-top: 0;
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
    gap: 6px;
    flex-wrap: wrap;
  }

  .toggle.inline {
    margin-bottom: 0;
    padding-bottom: 2px;
    font-size: 0.8rem;
    color: var(--text-menu);
  }

  .actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    margin-bottom: 6px;
    align-items: center;
  }

  .status {
    margin-bottom: 8px;
    padding: 6px 10px;
    border-radius: 8px;
    font-size: 0.8rem;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
  }

  .status.compact {
    margin-bottom: 8px;
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