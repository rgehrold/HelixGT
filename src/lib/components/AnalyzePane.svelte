<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import InfoLink from "$lib/components/InfoLink.svelte";
  import {
    runFastqQc,
    viewGetReadsInRange,
    viewOpenAlignment,
    viewOpenAnnotation,
    viewOpenDocument,
  } from "$lib/api";
  import type {
    AlignmentDocument,
    AlignmentRead,
    AnnotationDocument,
    FastqQcSummary,
    LogLevel,
    SequenceDocument,
  } from "$lib/types";

  interface Props {
    selectedPaths: string[];
    referencePath?: string;
    disabled?: boolean;
    onLog: (message: string, level?: LogLevel) => void;
  }

  let {
    selectedPaths,
    referencePath = $bindable(""),
    disabled = false,
    onLog,
  }: Props = $props();

  type Kind =
    | "none"
    | "fastq"
    | "fasta"
    | "alignment"
    | "annotation"
    | "vcf"
    | "genbank"
    | "unknown";

  let isBusy = $state(false);
  let errorMessage = $state("");
  let statusMessage = $state("Select a file in the Input browser, then run analysis.");

  let seqDoc = $state<SequenceDocument | null>(null);
  let annDoc = $state<AnnotationDocument | null>(null);
  let alignDoc = $state<AlignmentDocument | null>(null);
  let fastqQc = $state<FastqQcSummary | null>(null);

  let contig = $state("");
  let regionStart = $state(1); // 1-based UI
  let regionEnd = $state(10_000);
  let hideSecondary = $state(true);
  let hideSupplementary = $state(true);
  let hideDuplicates = $state(true);
  let minMapq = $state(0);
  let readFilter = $state("");
  let alignmentReads = $state<AlignmentRead[]>([]);
  let readsTruncated = $state(false);
  let readsTotal = $state(0);
  let selectedRead = $state<AlignmentRead | null>(null);
  let isLoadingReads = $state(false);

  const primaryPath = $derived(selectedPaths[0] ?? null);
  const kind = $derived(detectKind(primaryPath));
  const needsCramRef = $derived(kind === "alignment" && !!primaryPath && /\.cram$/i.test(primaryPath));
  const cramRefReady = $derived(referencePath.trim().length > 0);

  const contigOptions = $derived.by(() => {
    if (alignDoc) return alignDoc.contigs.map((c) => ({ name: c.name, length: c.length }));
    if (seqDoc) return seqDoc.contigs.map((c) => ({ name: c.name, length: c.length }));
    if (annDoc) {
      return (annDoc.contigSpans ?? []).map((s) => ({ name: s.name, length: s.length }));
    }
    return [] as { name: string; length: number }[];
  });

  const currentContigLength = $derived(
    contigOptions.find((c) => c.name === contig)?.length ?? 0,
  );

  const filteredReads = $derived.by(() => {
    const q = readFilter.trim().toLowerCase();
    if (!q) return alignmentReads;
    return alignmentReads.filter(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        (r.cigar ?? "").toLowerCase().includes(q) ||
        r.strand.includes(q) ||
        String(r.mapq).includes(q) ||
        flagSummary(r).toLowerCase().includes(q),
    );
  });

  const mapqHistogram = $derived.by(() => {
    const buckets = [
      { label: "0", min: 0, max: 0, count: 0 },
      { label: "1–9", min: 1, max: 9, count: 0 },
      { label: "10–19", min: 10, max: 19, count: 0 },
      { label: "20–29", min: 20, max: 29, count: 0 },
      { label: "30–39", min: 30, max: 39, count: 0 },
      { label: "40–60", min: 40, max: 60, count: 0 },
    ];
    for (const r of alignmentReads) {
      const b = buckets.find((x) => r.mapq >= x.min && r.mapq <= x.max);
      if (b) b.count += 1;
    }
    return buckets;
  });

  const flagStats = $derived.by(() => {
    let primary = 0;
    let secondary = 0;
    let supp = 0;
    let dup = 0;
    let reverse = 0;
    let paired = 0;
    let proper = 0;
    for (const r of alignmentReads) {
      if (r.isSecondary) secondary += 1;
      else if (r.isSupplementary) supp += 1;
      else primary += 1;
      if (r.isDuplicate) dup += 1;
      if (r.isReverse) reverse += 1;
      if (r.isPaired) paired += 1;
      if (r.isProperPair) proper += 1;
    }
    return { primary, secondary, supp, dup, reverse, paired, proper, total: alignmentReads.length };
  });

  function detectKind(path: string | null): Kind {
    if (!path) return "none";
    if (/\.(fastq|fq)(\.gz)?$/i.test(path)) return "fastq";
    if (/\.(fasta|fa|fna|ffn|frn)(\.gz)?$/i.test(path)) return "fasta";
    if (/\.(bam|cram|sam)(\.gz)?$/i.test(path)) return "alignment";
    if (/\.(gff|gff3|bed)(\.gz)?$/i.test(path)) return "annotation";
    if (/\.vcf(\.gz)?$/i.test(path)) return "vcf";
    if (/\.(gb|gbk|gbff)(\.gz)?$/i.test(path)) return "genbank";
    return "unknown";
  }

  function kindLabel(k: Kind): string {
    switch (k) {
      case "fastq":
        return "FASTQ reads";
      case "fasta":
        return "FASTA sequence";
      case "alignment":
        return "Alignment (SAM/BAM/CRAM)";
      case "annotation":
        return "Annotation (GFF/BED)";
      case "vcf":
        return "Variants (VCF)";
      case "genbank":
        return "GenBank";
      case "unknown":
        return "Unknown / unsupported";
      default:
        return "No file selected";
    }
  }

  function basename(path: string) {
    return path.split(/[\\/]/).pop() ?? path;
  }

  function resetResults() {
    seqDoc = null;
    annDoc = null;
    alignDoc = null;
    fastqQc = null;
    alignmentReads = [];
    readsTruncated = false;
    readsTotal = 0;
    selectedRead = null;
    errorMessage = "";
  }

  async function browseReferenceFasta() {
    const picked = await open({
      directory: false,
      multiple: false,
      title: "Choose reference FASTA for CRAM",
      filters: [{ name: "FASTA", extensions: ["fasta", "fa", "fna", "fa.gz", "fasta.gz", "fna.gz"] }],
    });
    if (picked) {
      referencePath = String(picked);
      onLog(`Reference set to ${referencePath}`);
    }
  }

  async function runAnalysis() {
    if (!primaryPath) {
      onLog("Select a file in the Input browser first.", "error");
      return;
    }
    if (needsCramRef && !cramRefReady) {
      errorMessage =
        "CRAM requires a reference FASTA. Choose one below or right-click a FASTA → Set as reference.";
      onLog("Cannot analyze CRAM without a reference FASTA.", "error");
      return;
    }

    isBusy = true;
    resetResults();
    statusMessage = `Analyzing ${basename(primaryPath)}…`;
    onLog(`File analysis: ${primaryPath}`);

    try {
      switch (kind) {
        case "fastq":
          await analyzeFastq(primaryPath);
          break;
        case "fasta":
          await analyzeFasta(primaryPath);
          break;
        case "annotation":
          await analyzeAnnotation(primaryPath);
          break;
        case "alignment":
          await analyzeAlignment(primaryPath);
          break;
        case "vcf":
          statusMessage = "VCF detected — basic identity only (no dedicated VCF stats yet).";
          onLog("VCF analysis is limited to file identity for now.", "warn");
          break;
        case "genbank":
          statusMessage = "GenBank detected — convert to FASTA for sequence stats, or use Convert.";
          onLog("GenBank: use Convert → FASTA for sequence extraction.", "info");
          break;
        default:
          errorMessage = "This file type is not recognized for analysis.";
          statusMessage = "Unsupported file.";
          onLog(`Unsupported for analysis: ${primaryPath}`, "warn");
      }
    } catch (error) {
      errorMessage = String(error);
      statusMessage = "Analysis failed.";
      onLog(`Analysis failed: ${String(error)}`, "error");
    } finally {
      isBusy = false;
    }
  }

  async function analyzeFastq(path: string) {
    fastqQc = await runFastqQc(path);
    statusMessage = `FASTQ QC complete — ${fastqQc.readCount.toLocaleString()} reads.`;
    onLog(
      `FASTQ QC: ${fastqQc.readCount.toLocaleString()} reads, mean length ${fastqQc.meanReadLength.toFixed(1)}, N-content ${(fastqQc.nContentFraction * 100).toFixed(3)}%.`,
    );
  }

  async function analyzeFasta(path: string) {
    seqDoc = await viewOpenDocument(path);
    contig = seqDoc.contigs[0]?.name ?? "";
    statusMessage = `FASTA — ${seqDoc.contigs.length} contig(s), ${seqDoc.totalBases.toLocaleString()} bases.`;
    onLog(
      `Sequence analysis: ${seqDoc.contigs.length} contig(s), ${seqDoc.totalBases.toLocaleString()} bases.`,
    );
  }

  async function analyzeAnnotation(path: string) {
    annDoc = await viewOpenAnnotation(path);
    statusMessage = `${annDoc.format.toUpperCase()} — ${annDoc.featureCount.toLocaleString()} feature(s), ${annDoc.contigs.length} contig(s).`;
    onLog(
      `Annotation analysis: ${annDoc.featureCount.toLocaleString()} features on ${annDoc.contigs.length} contig(s).`,
    );
  }

  async function analyzeAlignment(path: string) {
    const isCram = /\.cram$/i.test(path);
    if (/\.sam(\.gz)?$/i.test(path)) {
      statusMessage =
        "SAM is text; for region read tables prefer sorted+indexed BAM/CRAM. Opening may still work depending on backend support.";
    }
    alignDoc = await viewOpenAlignment({
      path,
      referencePath: isCram ? referencePath.trim() : null,
    });
    contig = alignDoc.contigs[0]?.name ?? "";
    const len = alignDoc.contigs[0]?.length ?? 10_000;
    regionStart = 1;
    regionEnd = Math.min(10_000, Math.max(1, len));
    statusMessage = `${alignDoc.format.toUpperCase()} — ${alignDoc.contigs.length} contig(s)${alignDoc.indexed ? ", indexed" : " (no index detected)"}.`;
    onLog(
      `Alignment analysis: ${alignDoc.format.toUpperCase()}, ${alignDoc.contigs.length} contigs, indexed=${alignDoc.indexed}.`,
    );
    if (alignDoc.indexed && contig) {
      await loadReads();
    }
  }

  async function loadReads() {
    if (!alignDoc || !contig) {
      onLog("Open an alignment and pick a contig first.", "warn");
      return;
    }
    const start0 = Math.max(0, Math.floor(regionStart) - 1);
    let end0 = Math.max(start0 + 1, Math.floor(regionEnd));
    if (currentContigLength > 0) {
      end0 = Math.min(end0, currentContigLength);
    }
    if (end0 - start0 > 500_000) {
      onLog("Region wider than 500 kb — loading may be truncated. Narrow the range.", "warn");
    }

    isLoadingReads = true;
    selectedRead = null;
    try {
      const ref =
        alignDoc.requiresReference && referencePath.trim() ? referencePath.trim() : null;
      const result = await viewGetReadsInRange({
        path: alignDoc.path,
        contig,
        start: start0,
        end: end0,
        referencePath: ref,
        includeSecondary: !hideSecondary,
        includeSupplementary: !hideSupplementary,
        includeDuplicates: !hideDuplicates,
        minMapq,
      });
      alignmentReads = result.reads;
      readsTruncated = result.truncated;
      readsTotal = result.totalInRange;
      statusMessage = `Loaded ${result.reads.length.toLocaleString()} read(s) in ${contig}:${start0 + 1}–${end0}${result.truncated ? " (truncated)" : ""}.`;
      onLog(statusMessage);
    } catch (error) {
      errorMessage = String(error);
      onLog(`Read load failed: ${String(error)}`, "error");
    } finally {
      isLoadingReads = false;
    }
  }

  function onContigChange(name: string) {
    contig = name;
    const len = contigOptions.find((c) => c.name === name)?.length ?? 10_000;
    regionStart = 1;
    regionEnd = Math.min(10_000, Math.max(1, len));
    alignmentReads = [];
    selectedRead = null;
  }

  function flagSummary(r: AlignmentRead): string {
    const tags: string[] = [];
    if (r.isPaired) tags.push(r.isProperPair ? "proper pair" : "paired");
    if (r.isFirstInPair) tags.push("R1");
    if (r.isSecondInPair) tags.push("R2");
    if (r.isReverse) tags.push("rev");
    if (r.isSecondary) tags.push("secondary");
    if (r.isSupplementary) tags.push("supp");
    if (r.isDuplicate) tags.push("dup");
    if (r.isQcFail) tags.push("QC fail");
    if (r.isMateUnmapped) tags.push("mate unmapped");
    return tags.join(", ") || "primary";
  }

  function meanQuality(r: AlignmentRead): string {
    if (!r.qualities || r.qualities === "*") return "—";
    let sum = 0;
    let n = 0;
    for (let i = 0; i < r.qualities.length; i++) {
      sum += r.qualities.charCodeAt(i) - 33;
      n++;
    }
    if (n === 0) return "—";
    return (sum / n).toFixed(1);
  }

  function readKey(r: AlignmentRead) {
    return `${r.name}|${r.start}|${r.flags}`;
  }

  function copyReadsTsv() {
    const header = [
      "name",
      "start",
      "end",
      "strand",
      "mapq",
      "cigar",
      "flags",
      "flag_summary",
      "tlen",
    ].join("\t");
    const lines = filteredReads.map((r) =>
      [
        r.name,
        r.start + 1,
        r.end,
        r.strand,
        r.mapq,
        r.cigar ?? "",
        r.flags,
        flagSummary(r),
        r.isPaired ? r.templateLength : "",
      ].join("\t"),
    );
    const text = [header, ...lines].join("\n");
    void navigator.clipboard.writeText(text).then(
      () => onLog(`Copied ${lines.length} read row(s) to clipboard.`),
      () => onLog("Could not copy to clipboard.", "error"),
    );
  }

  function exportReadsTsv() {
    const header = [
      "name",
      "start",
      "end",
      "strand",
      "mapq",
      "cigar",
      "flags",
      "flag_summary",
      "tlen",
    ].join("\t");
    const lines = filteredReads.map((r) =>
      [
        r.name,
        r.start + 1,
        r.end,
        r.strand,
        r.mapq,
        r.cigar ?? "",
        r.flags,
        flagSummary(r),
        r.isPaired ? r.templateLength : "",
      ].join("\t"),
    );
    const blob = new Blob([[header, ...lines].join("\n")], { type: "text/tab-separated-values" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${basename(alignDoc?.path ?? "reads")}_region.tsv`;
    a.click();
    URL.revokeObjectURL(url);
    onLog(`Exported ${lines.length} read row(s).`);
  }
</script>

<div class="pane-body">
  <div class="card">
    <div class="card-head">
      <strong>Selection</strong>
      <InfoLink section="analyze" label="File analysis — manual" />
    </div>
    {#if primaryPath}
      <div class="identity">
        <div><span class="k">File</span><span class="v mono" title={primaryPath}>{basename(primaryPath)}</span></div>
        <div><span class="k">Kind</span><span class="v">{kindLabel(kind)}</span></div>
        {#if selectedPaths.length > 1}
          <div>
            <span class="k">Note</span>
            <span class="v">Using first of {selectedPaths.length} selected paths</span>
          </div>
        {/if}
      </div>
    {:else}
      <p class="subtle">Select a file in the Input browser (left).</p>
    {/if}

    {#if needsCramRef}
      <div class="field cram-ref">
        <div class="label-with-help">
          <span>Reference FASTA (required for CRAM)</span>
          <InfoLink section="cram-and-reference" label="CRAM reference — manual" />
        </div>
        <div class="row">
          <button class="ghost" onclick={browseReferenceFasta} disabled={disabled || isBusy}>Choose</button>
          <input
            bind:value={referencePath}
            placeholder="C:\path\to\reference.fasta"
            disabled={disabled || isBusy}
          />
        </div>
        <p class="subtle">
          Or right-click a FASTA → <strong>Set as reference</strong>.
        </p>
      </div>
    {/if}

    <div class="actions">
      <button
        class="primary"
        onclick={runAnalysis}
        disabled={disabled || isBusy || !primaryPath || (needsCramRef && !cramRefReady)}
      >
        {isBusy ? "Analyzing…" : "Analyze selected"}
      </button>
    </div>
  </div>

  {#if errorMessage}
    <div class="status bad">{errorMessage}</div>
  {:else}
    <p class="status-line">{statusMessage}</p>
  {/if}

  {#if fastqQc}
    <div class="card">
      <div class="card-head"><strong>FASTQ QC</strong></div>
      <div class="stats-grid">
        <div class="stat"><span class="k">Reads</span><span class="v mono">{fastqQc.readCount.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Total bases</span><span class="v mono">{fastqQc.totalBases.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Mean length</span><span class="v mono">{fastqQc.meanReadLength.toFixed(2)}</span></div>
        <div class="stat"><span class="k">Min length</span><span class="v mono">{fastqQc.minReadLength.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Max length</span><span class="v mono">{fastqQc.maxReadLength.toLocaleString()}</span></div>
        <div class="stat"><span class="k">N content</span><span class="v mono">{(fastqQc.nContentFraction * 100).toFixed(4)}%</span></div>
      </div>
    </div>
  {/if}

  {#if seqDoc}
    <div class="card">
      <div class="card-head"><strong>Sequence summary</strong></div>
      <div class="stats-grid">
        <div class="stat"><span class="k">Format</span><span class="v">{seqDoc.format.toUpperCase()}{seqDoc.gzipped ? " · gz" : ""}</span></div>
        <div class="stat"><span class="k">Contigs</span><span class="v mono">{seqDoc.contigs.length.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Total bases</span><span class="v mono">{seqDoc.totalBases.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Cache</span><span class="v">{seqDoc.fullyCached ? "full in memory" : "on demand"}</span></div>
      </div>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Contig</th>
              <th>Length (bp)</th>
            </tr>
          </thead>
          <tbody>
            {#each seqDoc.contigs as c}
              <tr>
                <td class="mono">{c.name}</td>
                <td class="mono">{c.length.toLocaleString()}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  {/if}

  {#if annDoc}
    <div class="card">
      <div class="card-head"><strong>Annotation summary</strong></div>
      <div class="stats-grid">
        <div class="stat"><span class="k">Format</span><span class="v">{annDoc.format.toUpperCase()}{annDoc.gzipped ? " · gz" : ""}</span></div>
        <div class="stat"><span class="k">Features</span><span class="v mono">{annDoc.featureCount.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Contigs</span><span class="v mono">{annDoc.contigs.length.toLocaleString()}</span></div>
      </div>
      {#if annDoc.contigSpans?.length}
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Contig</th>
                <th>Span end (bp)</th>
              </tr>
            </thead>
            <tbody>
              {#each annDoc.contigSpans as s}
                <tr>
                  <td class="mono">{s.name}</td>
                  <td class="mono">{s.length.toLocaleString()}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>
  {/if}

  {#if alignDoc}
    <div class="card">
      <div class="card-head"><strong>Alignment summary</strong></div>
      <div class="stats-grid">
        <div class="stat"><span class="k">Format</span><span class="v">{alignDoc.format.toUpperCase()}</span></div>
        <div class="stat"><span class="k">Indexed</span><span class="v">{alignDoc.indexed ? "yes" : "no"}</span></div>
        <div class="stat"><span class="k">Contigs</span><span class="v mono">{alignDoc.contigs.length.toLocaleString()}</span></div>
        <div class="stat"><span class="k">Needs reference</span><span class="v">{alignDoc.requiresReference ? "yes (CRAM)" : "no"}</span></div>
      </div>
      {#if !alignDoc.indexed}
        <p class="subtle warn">
          No index detected. Region queries need a coordinate-sorted BAM/CRAM with .bai/.crai. Use Align
          with “Create index”, or index externally.
        </p>
      {/if}

      <div class="region-controls">
        <label class="field-inline">
          <span>Contig</span>
          <select
            value={contig}
            onchange={(e) => onContigChange((e.currentTarget as HTMLSelectElement).value)}
            disabled={disabled || isBusy || contigOptions.length === 0}
          >
            {#each contigOptions as c}
              <option value={c.name}>{c.name} ({c.length.toLocaleString()} bp)</option>
            {/each}
          </select>
        </label>
        <label class="field-inline narrow">
          <span>Start (1-based)</span>
          <input type="number" min="1" bind:value={regionStart} disabled={disabled || isBusy} />
        </label>
        <label class="field-inline narrow">
          <span>End (1-based)</span>
          <input type="number" min="1" bind:value={regionEnd} disabled={disabled || isBusy} />
        </label>
        <button class="ghost" onclick={loadReads} disabled={disabled || isBusy || isLoadingReads || !alignDoc.indexed}>
          {isLoadingReads ? "Loading…" : "Load reads in region"}
        </button>
      </div>

      <div class="filters">
        <label class="toggle">
          <input type="checkbox" bind:checked={hideSecondary} disabled={disabled} />
          <span>Hide secondary</span>
        </label>
        <label class="toggle">
          <input type="checkbox" bind:checked={hideSupplementary} disabled={disabled} />
          <span>Hide supplementary</span>
        </label>
        <label class="toggle">
          <input type="checkbox" bind:checked={hideDuplicates} disabled={disabled} />
          <span>Hide duplicates</span>
        </label>
        <label class="field-inline narrow">
          <span>Min MAPQ</span>
          <input type="number" min="0" max="60" bind:value={minMapq} disabled={disabled} />
        </label>
      </div>
    </div>

    {#if alignmentReads.length > 0 || isLoadingReads}
      <div class="card">
        <div class="card-head">
          <strong>
            Region reads
            {#if !isLoadingReads}
              — {filteredReads.length.toLocaleString()}
              {#if readsTruncated || readsTotal > alignmentReads.length}
                shown / {readsTotal.toLocaleString()} total
              {/if}
            {/if}
          </strong>
          <div class="head-actions">
            <input
              class="filter"
              type="search"
              placeholder="Filter name, CIGAR, MAPQ…"
              bind:value={readFilter}
              disabled={disabled}
            />
            <button class="ghost compact" onclick={copyReadsTsv} disabled={filteredReads.length === 0}>
              Copy TSV
            </button>
            <button class="ghost compact" onclick={exportReadsTsv} disabled={filteredReads.length === 0}>
              Export TSV
            </button>
          </div>
        </div>

        {#if alignmentReads.length > 0}
          <div class="stats-grid compact">
            <div class="stat"><span class="k">Primary</span><span class="v mono">{flagStats.primary}</span></div>
            <div class="stat"><span class="k">Secondary</span><span class="v mono">{flagStats.secondary}</span></div>
            <div class="stat"><span class="k">Supplementary</span><span class="v mono">{flagStats.supp}</span></div>
            <div class="stat"><span class="k">Duplicates</span><span class="v mono">{flagStats.dup}</span></div>
            <div class="stat"><span class="k">Reverse</span><span class="v mono">{flagStats.reverse}</span></div>
            <div class="stat"><span class="k">Paired / proper</span><span class="v mono">{flagStats.paired} / {flagStats.proper}</span></div>
          </div>

          <div class="mapq-row">
            <span class="k">MAPQ in loaded set</span>
            <div class="mapq-bars">
              {#each mapqHistogram as b}
                {@const maxC = Math.max(1, ...mapqHistogram.map((x) => x.count))}
                <div class="mapq-bar" title="{b.label}: {b.count}">
                  <div class="fill" style={`height:${(b.count / maxC) * 100}%`}></div>
                  <span class="lbl">{b.label}</span>
                  <span class="cnt">{b.count}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <div class="read-list">
          {#if isLoadingReads}
            <p class="subtle">Loading reads…</p>
          {:else if filteredReads.length === 0}
            <p class="subtle">No reads match this filter / region.</p>
          {:else}
            <table>
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Start</th>
                  <th>End</th>
                  <th>Len</th>
                  <th>Str</th>
                  <th>MAPQ</th>
                  <th>CIGAR</th>
                  <th>Flags</th>
                  <th>TLEN</th>
                </tr>
              </thead>
              <tbody>
                {#each filteredReads as read (readKey(read))}
                  <tr
                    class:selected={selectedRead != null && readKey(selectedRead) === readKey(read)}
                    onclick={() => (selectedRead = read)}
                  >
                    <td class="mono name" title={read.name}>{read.name}</td>
                    <td class="mono">{read.start + 1}</td>
                    <td class="mono">{read.end}</td>
                    <td class="mono">{read.end - read.start}</td>
                    <td class="mono">{read.strand}</td>
                    <td class="mono">{read.mapq}</td>
                    <td class="mono cigar" title={read.cigar}>{read.cigar}</td>
                    <td class="flags" title={flagSummary(read)}>{flagSummary(read)}</td>
                    <td class="mono">{read.isPaired ? read.templateLength : "—"}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
        {#if readsTruncated}
          <p class="subtle">List capped at server max — narrow the region for denser loci.</p>
        {/if}
      </div>
    {/if}

    {#if selectedRead}
      <div class="card inspector">
        <div class="card-head"><strong>Read: {selectedRead.name}</strong></div>
        <div class="stats-grid">
          <div class="stat"><span class="k">Locus</span><span class="v mono">{contig}:{selectedRead.start + 1}–{selectedRead.end}</span></div>
          <div class="stat"><span class="k">Length (ref)</span><span class="v mono">{selectedRead.end - selectedRead.start} bp</span></div>
          <div class="stat"><span class="k">Strand</span><span class="v mono">{selectedRead.strand}</span></div>
          <div class="stat"><span class="k">MAPQ</span><span class="v mono">{selectedRead.mapq}</span></div>
          <div class="stat"><span class="k">Flags</span><span class="v mono">0x{selectedRead.flags.toString(16)} ({selectedRead.flags})</span></div>
          <div class="stat"><span class="k">Flag summary</span><span class="v">{flagSummary(selectedRead)}</span></div>
          <div class="stat wide"><span class="k">CIGAR</span><span class="v mono wrap">{selectedRead.cigar}</span></div>
          <div class="stat"><span class="k">Mean base Q</span><span class="v mono">{meanQuality(selectedRead)}</span></div>
          <div class="stat"><span class="k">Paired</span><span class="v">{selectedRead.isPaired ? (selectedRead.isProperPair ? "yes (proper)" : "yes") : "no"}</span></div>
          <div class="stat"><span class="k">Template length</span><span class="v mono">{selectedRead.isPaired ? selectedRead.templateLength : "—"}</span></div>
          <div class="stat"><span class="k">Mate</span><span class="v mono">{selectedRead.mateContig ? `${selectedRead.mateContig}:${(selectedRead.mateStart ?? 0) + 1}` : "—"}</span></div>
          <div class="stat"><span class="k">Sec / supp / dup</span><span class="v mono">{selectedRead.isSecondary ? "Y" : "n"} / {selectedRead.isSupplementary ? "Y" : "n"} / {selectedRead.isDuplicate ? "Y" : "n"}</span></div>
        </div>
        {#if selectedRead.sequence}
          <div class="seq-block-wrap">
            <span class="k">Sequence ({selectedRead.sequence.length} bp)</span>
            <pre class="seq-block">{selectedRead.sequence}</pre>
          </div>
        {/if}
        {#if selectedRead.qualities && selectedRead.qualities !== "*"}
          <div class="seq-block-wrap">
            <span class="k">Qualities (Phred+33)</span>
            <pre class="seq-block">{selectedRead.qualities}</pre>
          </div>
        {/if}
      </div>
    {/if}
  {/if}

  {#if kind === "vcf" && primaryPath && !isBusy}
    <div class="card">
      <p class="subtle">
        Selected VCF: <code>{basename(primaryPath)}</code>. Use Convert/Merge for file ops; deep variant
        exploration is best in IGV/bcftools. Full VCF summary stats can be added later.
      </p>
    </div>
  {/if}
</div>

<style>
  .pane-body {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-height: 0;
  }

  .card {
    padding: 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--panel-border);
    background: var(--chip-bg);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .card.inspector {
    border-color: var(--chip-active-border);
    background: var(--success-bg);
  }

  .card-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }

  .card-head strong {
    font-size: 0.92rem;
    color: var(--text-primary);
  }

  .head-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  .identity,
  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
    gap: 10px 14px;
  }

  .stats-grid.compact {
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  }

  .stat.wide {
    grid-column: 1 / -1;
  }

  .k {
    display: block;
    color: var(--text-muted);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    margin-bottom: 2px;
  }

  .v {
    color: var(--text-primary);
    font-size: 0.86rem;
  }

  .mono {
    font-family: "JetBrains Mono", monospace;
  }

  .wrap {
    word-break: break-all;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .cram-ref {
    padding: 10px;
    border-radius: 12px;
    border: 1px solid var(--chip-border);
    background: var(--panel-bg);
  }

  .label-with-help {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-menu);
    font-size: 0.88rem;
    font-weight: 500;
  }

  .row {
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  .row > .ghost {
    flex: 0 0 auto;
  }

  .row > input {
    flex: 1 1 auto;
    min-width: 0;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  .region-controls,
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    align-items: flex-end;
  }

  .field-inline {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 140px;
  }

  .field-inline.narrow {
    min-width: 100px;
    max-width: 120px;
  }

  .field-inline span {
    color: var(--text-muted);
    font-size: 0.78rem;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-menu);
    font-size: 0.84rem;
  }

  input:not([type="checkbox"]),
  select,
  .filter {
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    font: inherit;
    box-sizing: border-box;
  }

  .filter {
    min-width: 180px;
    font-size: 0.82rem;
  }

  input:not([type="checkbox"]):focus,
  select:focus {
    outline: none;
    border-color: var(--accent-highlight);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-highlight) 55%, transparent);
  }

  .ghost,
  .primary {
    cursor: pointer;
    border: none;
    padding: 7px 12px;
    border-radius: 8px;
    font-weight: 600;
    font: inherit;
    font-size: 0.82rem;
  }

  .ghost.compact {
    padding: 4px 8px;
    font-size: 0.76rem;
  }

  .ghost {
    background: var(--chip-bg);
    color: var(--text-primary);
    border: 1px solid var(--chip-border);
  }

  .primary {
    background: var(--primary-bg);
    color: var(--primary-text);
    box-shadow: var(--primary-shadow);
  }

  .primary:disabled,
  .ghost:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .table-wrap,
  .read-list {
    max-height: 280px;
    overflow: auto;
    border-radius: 12px;
    border: 1px solid var(--panel-border);
    background: var(--log-surface-bg);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.8rem;
  }

  th {
    position: sticky;
    top: 0;
    background: var(--panel-bg);
    color: var(--text-muted);
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--panel-border);
    white-space: nowrap;
  }

  td {
    padding: 5px 8px;
    border-bottom: 1px solid var(--panel-border);
    color: var(--text-primary);
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .read-list tr {
    cursor: pointer;
  }

  .read-list tr:hover {
    background: var(--menu-hover-bg);
  }

  .read-list tr.selected {
    background: var(--chip-active-bg);
    color: var(--chip-active-text);
  }

  .read-list .name {
    max-width: 140px;
  }

  .read-list .cigar {
    max-width: 120px;
  }

  .read-list .flags {
    max-width: 160px;
    color: var(--text-muted);
  }

  .mapq-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .mapq-bars {
    display: flex;
    gap: 8px;
    align-items: flex-end;
    height: 72px;
  }

  .mapq-bar {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-end;
    height: 100%;
    gap: 2px;
  }

  .mapq-bar .fill {
    width: 100%;
    max-width: 36px;
    min-height: 2px;
    border-radius: 6px 6px 0 0;
    background: var(--chip-active-bg);
    border: 1px solid var(--chip-active-border);
  }

  .mapq-bar .lbl,
  .mapq-bar .cnt {
    font-size: 0.68rem;
    color: var(--text-muted);
  }

  .status {
    padding: 10px 12px;
    border-radius: 12px;
    font-size: 0.88rem;
  }

  .status.bad {
    border: 1px solid var(--status-error-border);
    background: var(--status-error-bg);
    color: var(--status-error-text);
  }

  .status-line {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.82rem;
  }

  .subtle {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.82rem;
    line-height: 1.45;
  }

  .subtle.warn {
    color: var(--warn);
  }

  .subtle code {
    font-family: "JetBrains Mono", monospace;
    color: var(--code-color);
  }

  .seq-block-wrap .k {
    margin-bottom: 4px;
  }

  .seq-block {
    margin: 0;
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--log-surface-bg);
    border: 1px solid var(--panel-border);
    color: var(--code-color);
    font-family: "JetBrains Mono", monospace;
    font-size: 0.76rem;
    line-height: 1.4;
    max-height: 120px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
