<script lang="ts">
  import { cancelJob, runAlignment as invokeAlignment, runPreflightCheck } from "$lib/api";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import HelpTip from "$lib/components/HelpTip.svelte";
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
    } catch (error) {
      referenceError = String(error);
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
    <div class="field-label">
      <span>Reference genome</span>
      <HelpTip
        label="Reference genome help"
        text="Download from NCBI RefSeq into a session cache, or pick a local FASTA. Alignment pipes minimap2 through samtools for valid headers, sorting, BAM/CRAM output, and indexing. For large whole-genome runs prefer CRAM."
      />
    </div>
    <select bind:value={selectedReferenceId} disabled={disabled || isFetching || isAligning}>
      {#each references as reference}
        <option value={reference.id}>
          {reference.label} (~{reference.compressedSizeMb} MB compressed)
        </option>
      {/each}
    </select>
    <div class="row">
      <button
        class="ghost"
        onclick={fetchReference}
        disabled={disabled || isFetching || isAligning || selectedReferenceId.startsWith("local:")}
      >
        {isFetching ? "Downloading…" : "Download from NCBI"}
      </button>
      <button class="ghost" onclick={browseLocalReference} disabled={disabled || isFetching || isAligning}>
        Local FASTA…
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

  <div class="field">
    <span>Read files ({readPaths.length} selected)</span>
    <p class="subtle">
      {#if readPaths.length === 0}
        No FASTA/FASTQ files selected in the browser.
      {:else}
        {readPaths.map((path) => path.split(/[\\/]/).pop()).join(", ")}
      {/if}
    </p>
  </div>

  <label class="field">
    <span>Output folder</span>
    <div class="row">
      <input
        value={outputDir}
        oninput={(event) => onOutputDirChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="C:\path\to\output"
        disabled={disabled || isAligning}
      />
      <button class="ghost" onclick={browseOutputDir} disabled={disabled || isAligning}>Choose</button>
    </div>
  </label>

  <label class="field">
    <span>Output basename</span>
    <input bind:value={outputStem} placeholder="aligned_reads" disabled={disabled || isAligning} />
  </label>

  <div class="field">
    <span>Output format</span>
    <div class="pills">
      <button class="pill" class:active={outputFormat === "sam"} onclick={() => (outputFormat = "sam")} disabled={disabled || isAligning}>
        SAM
      </button>
      <button class="pill" class:active={outputFormat === "bam"} onclick={() => (outputFormat = "bam")} disabled={disabled || isAligning}>
        BAM
      </button>
      <button class="pill" class:active={outputFormat === "cram"} onclick={() => (outputFormat = "cram")} disabled={disabled || isAligning}>
        CRAM
      </button>
    </div>
  </div>

  {#if outputFormat === "sam"}
    <label class="toggle">
      <input type="checkbox" bind:checked={compress} disabled={disabled || isAligning} />
      <span>Gzip compress SAM output (.sam.gz)</span>
      <HelpTip text="Only applies to SAM output. Writes a .sam.gz file instead of plain text SAM." />
    </label>
  {/if}

  <div class="field">
    <span>Minimap2 options</span>
    <label class="field-inline">
      <span class="label-with-help">
        <span>Read preset</span>
        <HelpTip text="Minimap2 -x preset. Use General for standard Illumina WGS. Short reads (-x sr) is for small genomes or short inserts, not typical 90 GB WGS runs." />
      </span>
      <select bind:value={preset} disabled={disabled || isAligning}>
        <option value="general">General</option>
        <option value="sr">Short reads (-x sr)</option>
        <option value="ont">Nanopore (-x map-ont)</option>
        <option value="hifi">PacBio HiFi (-x map-hifi)</option>
        <option value="splice">RNA-seq / splice (-x splice)</option>
        <option value="asm5">Assembly (-x asm5)</option>
      </select>
    </label>
    <label class="toggle">
      <input type="checkbox" bind:checked={indexReference} disabled={disabled || isAligning} />
      <span>Build reference index (.mmi) before aligning</span>
      <HelpTip text="Runs minimap2 -d to build a .mmi index first. Helps repeat alignments to the same reference; optional for one-off runs." />
    </label>
    <label class="toggle">
      <input type="checkbox" bind:checked={sortOutput} disabled={disabled || isAligning} />
      <span>Sort alignments by coordinate (samtools sort)</span>
      <HelpTip text="Sorts by chromosome and position. Required for IGV and most downstream tools." />
    </label>
    <label class="toggle">
      <input type="checkbox" bind:checked={secondaryAlignments} disabled={disabled || isAligning} />
      <span>Report secondary alignments</span>
      <HelpTip text="When off, minimap2 uses -N 0 and reports only primary alignments per read." />
    </label>
    {#if outputFormat !== "sam"}
      <label class="toggle">
        <input type="checkbox" bind:checked={indexOutput} disabled={disabled || isAligning} />
        <span>Create index (.bai / .crai) for IGV</span>
        <HelpTip text="Runs samtools index after BAM/CRAM output so IGV can load the file without building an index itself." />
      </label>
    {/if}
    <label class="toggle">
      <input type="checkbox" bind:checked={filterUnmapped} disabled={disabled || isAligning} />
      <span>Remove unmapped reads (samtools view -F 4)</span>
    </label>
    <label class="toggle">
      <input type="checkbox" bind:checked={markDuplicates} disabled={disabled || isAligning || outputFormat === "sam"} />
      <span>Mark/remove duplicates (samtools markdup)</span>
    </label>
  </div>

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
    gap: 8px;
    margin-bottom: 16px;
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

  .field-label,
  .label-with-help {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .field > span,
  .field-label > span,
  .toggle span {
    color: var(--text-menu);
    font-size: 0.92rem;
    font-weight: 500;
  }

  .toggle {
    flex-wrap: wrap;
  }

  .subtle {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.45;
    word-break: break-word;
  }

  select,
  input:not([type="checkbox"]) {
    width: 100%;
    padding: 11px 12px;
    border-radius: 12px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
  }

  .row {
    display: flex;
    gap: 8px;
  }

  .pills {
    display: flex;
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

  .pill:disabled,
  .ghost:disabled,
  .primary:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 4px;
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