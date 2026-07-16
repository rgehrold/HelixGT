<script lang="ts">
  import { cancelJob, runAlignment as invokeAlignment, runPreflightCheck } from "$lib/api";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import InfoLink from "$lib/components/InfoLink.svelte";
  import type {
    AlignSummary,
    EnsemblReferenceInfo,
    ReferenceDownloadProgress,
    ReferenceFetchResult,
  } from "$lib/types";

  interface Props {
    selectedPaths: string[];
    outputDir: string;
    disabled?: boolean;
    minimap2Available?: boolean;
    samtoolsAvailable?: boolean;
    onOutputDirChange: (value: string) => void;
    onLog: (message: string, level?: import("$lib/types").LogLevel) => void;
    onComplete: (outputPaths: string[]) => Promise<void>;
    onBusyChange?: (busy: boolean) => void;
  }

  let {
    selectedPaths,
    outputDir,
    disabled = false,
    minimap2Available = false,
    samtoolsAvailable = false,
    onOutputDirChange,
    onLog,
    onComplete,
    onBusyChange,
  }: Props = $props();

  let references = $state<EnsemblReferenceInfo[]>([]);
  let selectedReferenceId = $state("escherichia_coli_k12");
  let loadedReference = $state<ReferenceFetchResult | null>(null);
  let referenceError = $state("");
  let downloadProgress = $state<ReferenceDownloadProgress | null>(null);
  let outputStem = $state("aligned_reads");
  let outputFormat = $state<"sam" | "bam" | "cram">("cram");
  let compress = $state(false);
  let preset = $state("general");
  let indexReference = $state(false);
  let sortOutput = $state(true);
  let secondaryAlignments = $state(true);
  let indexOutput = $state(true);
  let filterUnmapped = $state(false);
  let markDuplicates = $state(false);
  let isFetching = $state(false);
  let currentJobId = $state<string | null>(null);
  let isSaving = $state(false);
  let isAligning = $state(false);
  let lastSummary = $state<AlignSummary | null>(null);

  const readPaths = $derived(
    selectedPaths.filter((path) => /\.(fasta|fa|fna|fastq|fq)(\.gz)?$/i.test(path)),
  );

  onMount(async () => {
    references = await invoke<EnsemblReferenceInfo[]>("list_ensembl_references");

    await listen<ReferenceDownloadProgress>("reference-download-progress", (event) => {
      if (event.payload.id === selectedReferenceId) {
        downloadProgress = event.payload;
      }
    });
  });

  async function browseOutputDir() {
    const picked = await open({
      directory: true,
      multiple: false,
      title: "Choose alignment output folder",
    });
    if (picked) onOutputDirChange(String(picked));
  }

  export async function loadReferenceFromPath(path: string) {
    isFetching = true;
    onBusyChange?.(true);
    referenceError = "";
    try {
      const id = `local:${path}`;
      loadedReference = await invoke<ReferenceFetchResult>("load_local_reference", {
        id,
        path,
      });
      selectedReferenceId = id;
      onLog(
        `Reference set — ${(loadedReference.sequenceBytes / 1_000_000).toFixed(1)} MB uncompressed.`,
      );
    } catch (error) {
      referenceError = String(error);
    } finally {
      isFetching = false;
      onBusyChange?.(false);
    }
  }

  async function browseLocalReference() {
    const picked = await open({
      directory: false,
      multiple: false,
      title: "Choose reference FASTA",
      filters: [{ name: "FASTA", extensions: ["fasta", "fa", "fna", "gz"] }],
    });
    if (!picked) return;
    await loadReferenceFromPath(String(picked));
  }

  async function fetchReference() {
    if (!selectedReferenceId || selectedReferenceId.startsWith("local:")) return;

    isFetching = true;
    onBusyChange?.(true);
    referenceError = "";
    downloadProgress = null;
    try {
      loadedReference = await invoke<ReferenceFetchResult>("fetch_ensembl_reference", {
        id: selectedReferenceId,
      });
      const sizeMb = (loadedReference.sequenceBytes / 1_000_000).toFixed(1);
      if (loadedReference.cached) {
        onLog(`Reference ready from cache — ${loadedReference.label} (${sizeMb} MB sequence).`);
      } else {
        onLog(`Reference downloaded — ${loadedReference.label} (${sizeMb} MB sequence).`);
      }
    } catch (error) {
      referenceError = String(error);
      onLog(`Reference download failed: ${String(error)}`, "error");
    } finally {
      isFetching = false;
      downloadProgress = null;
      onBusyChange?.(false);
    }
  }

  async function saveReference(decompress: boolean) {
    if (!loadedReference) return;
    const defaultName = loadedReference.label.replace(/[^\w.-]+/g, "_");
    const picked = await save({
      title: decompress ? "Save reference (FASTA)" : "Save reference (compressed)",
      defaultPath: decompress ? `${defaultName}.fasta` : `${defaultName}.fna.gz`,
      filters: decompress
        ? [{ name: "FASTA", extensions: ["fasta", "fa", "fna"] }]
        : [{ name: "Compressed FASTA", extensions: ["gz", "fna.gz"] }],
    });
    if (!picked) return;

    isSaving = true;
    onBusyChange?.(true);
    referenceError = "";
    try {
      await invoke("save_reference", {
        id: selectedReferenceId,
        path: String(picked),
        decompress,
      });
      onLog(`Reference saved to ${picked}`);
    } catch (error) {
      referenceError = String(error);
    } finally {
      isSaving = false;
      onBusyChange?.(false);
    }
  }

  async function clearReferenceCache() {
    await invoke("clear_reference_cache", { id: null });
    loadedReference = null;
    referenceError = "";
    onLog("Reference cache cleared.");
  }

  async function runAlignment() {
    lastSummary = null;

    if (!minimap2Available) {
      onLog("minimap2 is required. Place minimap2.exe in src-tauri/binaries/ or add it to PATH.", "error");
      return;
    }
    if (!samtoolsAvailable) {
      onLog("samtools is required. Place samtools.exe in src-tauri/binaries/ or add it to PATH.", "error");
      return;
    }
    if (readPaths.length === 0) {
      onLog("Select at least one FASTA or FASTQ read file.", "error");
      return;
    }
    if (!outputDir.trim()) {
      onLog("Choose an output folder.", "error");
      return;
    }
    if (!loadedReference) {
      onLog("Download or load a reference genome first.", "error");
      return;
    }
    const preflight = await runPreflightCheck({
      mode: "align",
      inputPaths: readPaths,
      outputDir: outputDir.trim(),
      referencePath: loadedReference ? selectedReferenceId : null,
    });
    if (!preflight.ok) {
      onLog(preflight.issues.map((issue) => issue.message).join("\n"), "error");
      return;
    }

    isAligning = true;
    currentJobId = `align-${Date.now()}`;
    onBusyChange?.(true);
    onLog(`Aligning ${readPaths.length} read file(s) with minimap2…`);

    try {
      const summary = await invokeAlignment({
        readPaths,
        outputDir: outputDir.trim(),
        outputStem: outputStem.trim() || "aligned_reads",
        outputFormat,
        compress,
        referenceId: selectedReferenceId,
        preset,
        indexReference,
        sortOutput,
        secondaryAlignments,
        indexOutput,
        filterUnmapped,
        markDuplicates,
        jobId: currentJobId,
      });
      lastSummary = summary;
      onLog(
        `Alignment complete — ${summary.mappedReads.toLocaleString()} / ${summary.totalReads.toLocaleString()} mapped (${summary.aligner}).`,
      );
      const revealPaths = [summary.outputPath];
      if (summary.indexPath) revealPaths.push(summary.indexPath);
      await onComplete(revealPaths);
    } catch (error) {
      onLog(`Alignment error: ${String(error)}`, "error");
    } finally {
      isAligning = false;
      currentJobId = null;
      onBusyChange?.(false);
    }
  }

  async function cancelAlignment() {
    if (!currentJobId) return;
    await cancelJob(currentJobId);
    onLog("Alignment cancelled.", "warn");
  }
</script>

<div class="pane-body">
  <div class="field">
    <span class="field-label">Reference <InfoLink section="align-reference" label="Reference genome — manual" /></span>
    <select bind:value={selectedReferenceId} disabled={disabled || isFetching || isAligning}>
      {#each references as reference}
        <option value={reference.id}>
          {reference.label} (~{reference.compressedSizeMb} MB)
        </option>
      {/each}
    </select>
    <div class="row">
      <button
        class="ghost compact"
        onclick={fetchReference}
        disabled={disabled || isFetching || isAligning || selectedReferenceId.startsWith("local:")}
      >
        {isFetching ? "Downloading…" : "Download"}
      </button>
      <button class="ghost compact" onclick={browseLocalReference} disabled={disabled || isFetching || isAligning}>
        Local…
      </button>
    </div>
    {#if isFetching}
      <div class="progress-wrap">
        <div
          class="progress-bar"
          style={`width: ${downloadProgress?.percent ?? (downloadProgress ? 8 : 4)}%`}
        ></div>
      </div>
      <p class="progress-label">
        {#if downloadProgress?.percent != null}
          {downloadProgress.percent.toFixed(1)}% · {(downloadProgress.downloadedBytes / 1_000_000).toFixed(1)} MB
          {#if downloadProgress.totalBytes}
            / {(downloadProgress.totalBytes / 1_000_000).toFixed(1)} MB
          {/if}
        {:else if downloadProgress}
          {(downloadProgress.downloadedBytes / 1_000_000).toFixed(1)} MB downloaded…
        {:else}
          Starting download…
        {/if}
      </p>
    {/if}
  </div>

  {#if loadedReference || referenceError}
    <div class="status" class:ok={loadedReference && !referenceError} class:bad={!!referenceError}>
      {#if referenceError}
        <strong>Reference error</strong>
        <p>{referenceError}</p>
      {:else if loadedReference}
        <strong>{loadedReference.label}</strong>
        <p>
          {((loadedReference.sequenceBytes ?? 0) / 1_000_000).toFixed(1)} MB sequence · session cache
          {#if loadedReference.cached}(reused){/if}
        </p>
      {/if}
      <div class="ref-actions">
        <button class="linkish" onclick={() => saveReference(true)} disabled={disabled || isAligning || isSaving || !loadedReference}>
          {isSaving ? "Saving…" : "Save"}
        </button>
        <button class="linkish" onclick={() => saveReference(false)} disabled={disabled || isAligning || isSaving || !loadedReference}>
          {isSaving ? "Saving…" : "Save compressed"}
        </button>
        <button class="linkish" onclick={clearReferenceCache} disabled={disabled || isAligning}>
          Clear from memory
        </button>
      </div>
    </div>
  {/if}

  <p class="subtle">
    Reads: {#if readPaths.length === 0}<em>none selected</em>{:else}{readPaths.length} file(s) — {readPaths.map((path) => path.split(/[\\/]/).pop()).join(", ")}{/if}
  </p>

  <div class="field">
    <span class="field-label">Output folder</span>
    <div class="row">
      <button class="ghost compact" onclick={browseOutputDir} disabled={disabled || isAligning}>Choose</button>
      <input
        value={outputDir}
        oninput={(event) => onOutputDirChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="Output folder"
        disabled={disabled || isAligning}
      />
    </div>
  </div>

  <div class="field-row">
    <div class="field grow">
      <span class="field-label">Basename</span>
      <input bind:value={outputStem} placeholder="aligned_reads" disabled={disabled || isAligning} />
    </div>
    <div class="field">
      <span class="field-label">Format</span>
      <div class="pills">
        <button class="pill" class:active={outputFormat === "sam"} onclick={() => (outputFormat = "sam")} disabled={disabled || isAligning}>SAM</button>
        <button class="pill" class:active={outputFormat === "bam"} onclick={() => (outputFormat = "bam")} disabled={disabled || isAligning}>BAM</button>
        <button class="pill" class:active={outputFormat === "cram"} onclick={() => (outputFormat = "cram")} disabled={disabled || isAligning}>CRAM</button>
      </div>
    </div>
  </div>

  <details class="options-panel">
    <summary>Alignment options</summary>
    <div class="options-body">
      <label class="field compact">
        <span class="field-label">Preset <InfoLink section="align-preset" label="Read preset — manual" /></span>
        <select bind:value={preset} disabled={disabled || isAligning}>
          <option value="general">General</option>
          <option value="sr">Short reads (-x sr)</option>
          <option value="ont">Nanopore (-x map-ont)</option>
          <option value="hifi">PacBio HiFi (-x map-hifi)</option>
          <option value="splice">RNA-seq / splice (-x splice)</option>
          <option value="asm5">Assembly (-x asm5)</option>
        </select>
      </label>
      {#if outputFormat === "sam"}
        <label class="toggle inline">
          <input type="checkbox" bind:checked={compress} disabled={disabled || isAligning} />
          <span>Gzip SAM (.sam.gz)</span>
        </label>
      {/if}
      <label class="toggle inline">
        <input type="checkbox" bind:checked={indexReference} disabled={disabled || isAligning} />
        <span>Build .mmi index first</span>
      </label>
      <label class="toggle inline">
        <input type="checkbox" bind:checked={sortOutput} disabled={disabled || isAligning} />
        <span>Sort by coordinate</span>
      </label>
      <label class="toggle inline">
        <input type="checkbox" bind:checked={secondaryAlignments} disabled={disabled || isAligning} />
        <span>Secondary alignments</span>
      </label>
      {#if outputFormat !== "sam"}
        <label class="toggle inline">
          <input type="checkbox" bind:checked={indexOutput} disabled={disabled || isAligning} />
          <span>Create .bai / .crai</span>
        </label>
      {/if}
      <label class="toggle inline">
        <input type="checkbox" bind:checked={filterUnmapped} disabled={disabled || isAligning} />
        <span>Drop unmapped</span>
      </label>
      <label class="toggle inline">
        <input type="checkbox" bind:checked={markDuplicates} disabled={disabled || isAligning || outputFormat === "sam"} />
        <span>Mark duplicates</span>
      </label>
    </div>
  </details>

  {#if !minimap2Available || !samtoolsAvailable}
    <p class="note">
      Alignment requires <code>minimap2.exe</code> and <code>samtools.exe</code> in
      <code>src-tauri/binaries/</code> or PATH.
    </p>
  {/if}

  <div class="row actions">
    <button
      class="primary"
      onclick={runAlignment}
      disabled={disabled || isAligning || readPaths.length === 0 || !loadedReference || !minimap2Available || !samtoolsAvailable}
    >
      {isAligning ? "Aligning…" : "Run alignment"}
    </button>
    {#if isAligning}
      <button class="ghost" onclick={cancelAlignment}>Cancel</button>
    {/if}
  </div>

  {#if lastSummary}
    <div class="success">
      <strong>Alignment complete</strong>
      <p>
        {lastSummary.mappedReads.toLocaleString()} / {lastSummary.totalReads.toLocaleString()} mapped ·
        {lastSummary.aligner}
        {#if lastSummary.indexPath}
          · index written
        {/if}
      </p>
    </div>
  {/if}
</div>

<style>
  .pane-body {
    display: flex;
    flex-direction: column;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 10px;
  }

  .field-inline {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field-inline > span {
    color: var(--text-muted);
    font-size: 0.82rem;
  }

  .label-with-help {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .field-label {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--text-menu);
    font-size: 0.78rem;
    font-weight: 600;
  }

  .field.compact {
    margin-bottom: 8px;
  }

  .field-row {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: flex-end;
    margin-bottom: 10px;
  }

  .field.grow {
    flex: 1;
    min-width: 140px;
    margin-bottom: 0;
  }

  .options-panel {
    margin-bottom: 10px;
    border: 1px solid var(--chip-border);
    border-radius: 8px;
    background: var(--chip-bg);
    padding: 0 8px;
  }

  .options-panel summary {
    cursor: pointer;
    padding: 7px 4px;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-menu);
    list-style: none;
  }

  .options-panel summary::-webkit-details-marker {
    display: none;
  }

  .options-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 0 4px 10px;
  }

  .subtle {
    margin: 0 0 8px;
    color: var(--text-muted);
    font-size: 0.74rem;
    line-height: 1.35;
    word-break: break-word;
  }

  select,
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

  select:focus,
  select:focus-visible,
  input:not([type="checkbox"]):focus,
  input:not([type="checkbox"]):focus-visible {
    outline: none;
    border-color: var(--accent-highlight);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-highlight) 55%, transparent);
    position: relative;
    z-index: 1;
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

  .pills {
    display: flex;
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

  .pill:disabled,
  .ghost:disabled,
  .primary:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .toggle.inline {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 0;
    font-size: 0.8rem;
    color: var(--text-menu);
    cursor: pointer;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 4px;
  }

  .toggle-control {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .status {
    margin-bottom: 14px;
    padding: 10px 12px;
    border-radius: 12px;
    font-size: 0.88rem;
    border: 1px solid var(--chip-border);
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

  .status p {
    margin: 6px 0 0;
    font-size: 0.82rem;
    white-space: pre-wrap;
  }

  .ref-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 8px;
  }

  .linkish {
    padding: 0;
    border: none;
    background: none;
    color: var(--link-color);
    cursor: pointer;
    font-size: 0.82rem;
    text-decoration: underline;
  }

  .linkish:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .progress-wrap {
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
    margin: 0;
    color: var(--text-muted);
    font-size: 0.8rem;
  }

  .note {
    margin: 0 0 12px;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .note code {
    font-family: "JetBrains Mono", monospace;
    font-size: 0.76rem;
    color: var(--code-color);
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