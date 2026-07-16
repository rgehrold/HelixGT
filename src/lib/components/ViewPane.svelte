<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import InfoLink from "$lib/components/InfoLink.svelte";
  import {
    viewGetCoverageBins,
    viewGetFeaturesInRange,
    viewGetReadsInRange,
    viewGetSequenceWindow,
    viewOpenAlignment,
    viewOpenAnnotation,
    viewOpenDocument,
  } from "$lib/api";
  import type {
    AlignmentDocument,
    AlignmentRead,
    AnnotationDocument,
    AnnotationFeature,
    CoverageBin,
    LogLevel,
    SequenceDocument,
    SequenceSlice,
  } from "$lib/types";
  import {
    BASE_LETTERS_MAX_BP,
    FEATURE_FETCH_DEBOUNCE_MS,
    FETCH_DEBOUNCE_IDLE_MS,
    FETCH_DEBOUNCE_PAN_MS,
    GUTTER_WIDTH,
    KEY_PAN_FRACTION,
    MAX_SEQUENCE_WINDOW_BP,
    MIN_VISIBLE_BP,
    PAD_L,
    PAD_R,
    READ_FETCH_MAX_BP,
    READ_LANE_H,
    SEQ_PREFETCH_CHUNK_BP,
    SEQ_PREFETCH_PAD_BP,
    ZOOM_IN_FACTOR,
    ZOOM_OUT_FACTOR,
  } from "$lib/view/constants";
  import {
    cacheCoversWindow,
    cacheNearEdge,
    computeCoverageFetchWindow,
    computeReadFetchWindow,
    coverageCacheCovers,
    coverageNearEdge,
  } from "$lib/view/fetch";
  import {
    computeUnifiedLayout,
    defaultVisibleBp,
    estimateFeatureLanes,
    genomicFromClientX,
    hitOverview,
  } from "$lib/view/geometry";
  import { formatLocus, parseLocus } from "$lib/view/locus";
  import {
    flagSummary,
    meanQuality,
    packReadsSquish,
    readFilterKey,
    readsOverlapWindow,
    type PackedRead,
  } from "$lib/view/reads";
  import { paintFixedTracks, paintReadsTrack, type LocalBuffer } from "$lib/view/paint";

  interface Props {
    selectedPaths: string[];
    openPathRequest?: string | null;
    referencePath?: string;
    disabled?: boolean;
    onLog: (message: string, level?: LogLevel) => void;
    onOpenPathConsumed?: () => void;
  }

  let {
    selectedPaths,
    openPathRequest = null,
    referencePath = $bindable(""),
    disabled = false,
    onLog,
    onOpenPathConsumed,
  }: Props = $props();

  type ReadCache = {
    path: string;
    contig: string;
    start: number;
    end: number;
    filterKey: string;
    reads: AlignmentRead[];
    totalInRange: number;
    truncated: boolean;
  };

  type CoverageCache = {
    path: string;
    contig: string;
    start: number;
    end: number;
    binCount: number;
    bins: CoverageBin[];
    maxDepth: number;
  };

  type AnnTrackState = {
    path: string;
    label: string;
    visible: boolean;
    features: AnnotationFeature[];
    truncated: boolean;
    totalInRange: number;
  };

  let seqDoc = $state<SequenceDocument | null>(null);
  let annDocs = $state<AnnotationDocument[]>([]);
  let annTrackStates = $state<AnnTrackState[]>([]);
  let alignDoc = $state<AlignmentDocument | null>(null);
  let selectedFeature = $state<AnnotationFeature | null>(null);
  let coverageBins = $state<CoverageBin[]>([]);
  let coverageMax = $state(0);
  let alignmentReads = $state<AlignmentRead[]>([]);
  let readsTruncated = $state(false);
  let readsTotal = $state(0);
  let readCache = $state<ReadCache | null>(null);
  let coverageCache = $state<CoverageCache | null>(null);
  let selectedRead = $state<AlignmentRead | null>(null);
  let selectedCoverage = $state<CoverageBin | null>(null);

  let showCoverage = $state(true);
  let showSeqTrackVisible = $state(true);
  let showReadPileup = $state(true);
  let hideSecondary = $state(true);
  let hideSupplementary = $state(true);
  let hideDuplicates = $state(true);
  let minMapq = $state(0);
  let colorReadsBy = $state<"strand" | "mapq" | "pair" | "none">("strand");
  let colorMismatches = $state(false);
  let colorBases = $state(false);
  let reverseComplement = $state(false);

  let contig = $state("");
  let viewStart = $state(0);
  let visibleBp = $state(100);
  let slice = $state<SequenceSlice | null>(null);

  let isLoading = $state(false);
  let isFetchingSlice = $state(false);
  let isFetchingFeatures = $state(false);
  let isFetchingAlign = $state(false);
  let errorMessage = $state("");
  let statusMessage = $state("Open FASTA, GFF/BED, and/or indexed BAM/CRAM.");
  let pendingCramPath = $state<string | null>(null);

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let readsCanvasEl = $state<HTMLCanvasElement | null>(null);
  let wrapEl = $state<HTMLDivElement | null>(null);
  let locusInput = $state("");
  let locusEditing = $state(false);
  let openMenu = $state<"display" | "filters" | null>(null);

  let selectionStart = $state<number | null>(null);
  let selectionEnd = $state<number | null>(null);
  let dragOrigin = $state<number | null>(null);
  let isPanning = $state(false);
  let isOverviewDragging = $state(false);
  let panOriginX = $state(0);
  let panOriginStart = $state(0);

  let fetchToken = 0;
  let featureFetchToken = 0;
  let alignFetchToken = 0;
  let localBuffer: LocalBuffer | null = null;
  let prefetchTimer: ReturnType<typeof setTimeout> | null = null;
  let alignDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let featureDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let paintRaf = 0;

  const contigLength = $derived.by(() => {
    const fromSeq = seqDoc?.contigs.find((c) => c.name === contig)?.length;
    if (fromSeq && fromSeq > 0) return fromSeq;
    const fromAln = alignDoc?.contigs.find((c) => c.name === contig)?.length;
    if (fromAln && fromAln > 0) return fromAln;
    let max = 0;
    for (const doc of annDocs) {
      const span = doc.contigSpans?.find((s) => s.name === contig);
      if (span) max = Math.max(max, span.length);
    }
    return max;
  });
  const viewEnd = $derived(Math.min(viewStart + visibleBp, contigLength || viewStart + visibleBp));

  const selectedSequencePath = $derived(
    selectedPaths.find((path) => /\.(fasta|fa|fna|ffn|frn|fastq|fq)(\.gz)?$/i.test(path)) ?? null,
  );
  const selectedAnnotationPaths = $derived(
    selectedPaths.filter((path) => /\.(gff|gff3|bed)(\.gz)?$/i.test(path)),
  );
  const selectedAlignmentPath = $derived(
    selectedPaths.find((path) => /\.(bam|cram)$/i.test(path)) ?? null,
  );
  const selectedIsCram = $derived(!!selectedAlignmentPath && /\.cram$/i.test(selectedAlignmentPath));
  const openIsCram = $derived(
    !!alignDoc && (alignDoc.requiresReference || /\.cram$/i.test(alignDoc.path)),
  );
  const showCramReferenceField = $derived(selectedIsCram || openIsCram || !!pendingCramPath);
  const cramReferenceReady = $derived(referencePath.trim().length > 0);

  const contigOptions = $derived.by(() => {
    if (seqDoc) return seqDoc.contigs.map((c) => ({ name: c.name, length: c.length }));
    if (alignDoc) return alignDoc.contigs.map((c) => ({ name: c.name, length: c.length }));
    const map = new Map<string, number>();
    for (const doc of annDocs) {
      for (const span of doc.contigSpans ?? []) {
        map.set(span.name, Math.max(map.get(span.name) ?? 0, span.length));
      }
    }
    return [...map.entries()].map(([name, length]) => ({ name, length }));
  });

  const hasDocument = $derived(!!seqDoc || annDocs.length > 0 || !!alignDoc);
  const showSeqTrack = $derived(!!seqDoc && showSeqTrackVisible);
  const showCovTrack = $derived(!!alignDoc && showCoverage);
  const showReadsTrack = $derived(!!alignDoc && showReadPileup);
  const plotWidth = $derived(Math.max(200, (wrapEl?.clientWidth ?? 640) - GUTTER_WIDTH));

  const readPack = $derived.by(() =>
    packReadsSquish(alignmentReads, viewStart, viewEnd),
  );

  $effect(() => {
    if (locusEditing || !contig) return;
    viewStart;
    viewEnd;
    locusInput = formatLocus(contig, viewStart, viewEnd);
  });

  $effect(() => {
    if (openPathRequest) {
      void openAnyPath(openPathRequest);
      onOpenPathConsumed?.();
    }
  });

  // Viewport / filter-driven data load. Do NOT read annTrackStates here —
  // feature results write that array and would re-trigger an infinite fetch loop
  // (UI stuck on "fetching…" while hammering IPC).
  let lastRc = reverseComplement;
  $effect(() => {
    if (!hasDocument || !contig || contigLength <= 0) return;
    viewStart;
    visibleBp;
    reverseComplement;
    showReadPileup;
    showCoverage;
    showSeqTrackVisible;
    hideSecondary;
    hideSupplementary;
    hideDuplicates;
    minMapq;
    // Track annotation *document* list (open/close), not per-track feature arrays.
    annDocs;
    if (reverseComplement !== lastRc) {
      lastRc = reverseComplement;
      localBuffer = null;
    }
    if (seqDoc) refreshVisibleSlice();
    if (featureDebounceTimer) clearTimeout(featureDebounceTimer);
    featureDebounceTimer = setTimeout(() => void refreshFeatures(), FEATURE_FETCH_DEBOUNCE_MS);
    refreshAlignmentTracksDebounced();
  });

  $effect(() => {
    slice;
    annTrackStates;
    selectedFeature;
    selectedRead;
    selectedCoverage;
    coverageBins;
    alignmentReads;
    readPack;
    viewStart;
    visibleBp;
    contigLength;
    selectionStart;
    selectionEnd;
    reverseComplement;
    showCoverage;
    showSeqTrackVisible;
    showReadPileup;
    showSeqTrack;
    showCovTrack;
    showReadsTrack;
    colorReadsBy;
    colorMismatches;
    colorBases;
    plotWidth;
    schedulePaint();
  });

  // Auto-open pending CRAM when reference is set.
  let lastAutoRef = "";
  $effect(() => {
    const ref = referencePath.trim();
    if (!ref || ref === lastAutoRef || !pendingCramPath || isLoading) return;
    lastAutoRef = ref;
    void openAlignmentPath(pendingCramPath);
  });

  onMount(() => {
    const onResize = () => schedulePaint();
    window.addEventListener("resize", onResize);
    return () => {
      window.removeEventListener("resize", onResize);
      if (prefetchTimer) clearTimeout(prefetchTimer);
      if (alignDebounceTimer) clearTimeout(alignDebounceTimer);
      if (featureDebounceTimer) clearTimeout(featureDebounceTimer);
      if (paintRaf) cancelAnimationFrame(paintRaf);
    };
  });

  function schedulePaint() {
    if (paintRaf) return;
    // Paint on the next frame only — skip await tick() so pan stays fluid.
    paintRaf = requestAnimationFrame(() => {
      paintRaf = 0;
      paint();
    });
  }

  function clampWindow() {
    if (contigLength <= 0) {
      viewStart = 0;
      visibleBp = MIN_VISIBLE_BP;
      return;
    }
    visibleBp = Math.max(MIN_VISIBLE_BP, Math.min(visibleBp, contigLength));
    viewStart = Math.max(0, Math.min(viewStart, Math.max(0, contigLength - visibleBp)));
  }

  const viewerLayout = $derived.by(() =>
    computeUnifiedLayout(plotWidth, {
      showSeq: showSeqTrack,
      showCov: showCovTrack,
      annTracks: annTrackStates.map((t) => ({
        path: t.path,
        label: t.label,
        visible: t.visible,
        laneCount: estimateFeatureLanes(t.features, viewStart, viewEnd),
      })),
      showReads: showReadsTrack,
      readLaneCount: readPack.laneCount,
    }),
  );

  function paint() {
    if (!canvasEl) return;
    const pack = readPack;
    const layout = computeUnifiedLayout(plotWidth, {
      showSeq: showSeqTrack,
      showCov: showCovTrack,
      annTracks: annTrackStates.map((t) => ({
        path: t.path,
        label: t.label,
        visible: t.visible,
        laneCount: estimateFeatureLanes(t.features, viewStart, viewEnd),
      })),
      showReads: showReadsTrack,
      readLaneCount: pack.laneCount,
    });
    const cssW = plotWidth;
    paintFixedTracks({
      canvas: canvasEl,
      layout,
      cssW,
      viewStart,
      viewEnd,
      visibleBp,
      contig,
      contigLength,
      slice,
      localBuffer,
      isFetchingSlice,
      coverageBins,
      coverageMax,
      isFetchingAlign,
      annTracks: annTrackStates
        .filter((t) => t.visible)
        .map((t) => ({
          path: t.path,
          features: t.features,
          truncated: t.truncated,
        })),
      alignmentReads,
      selectedFeature,
      selectedCoverage,
      selectionStart,
      selectionEnd,
      colorMismatches,
      colorBases,
    });

    if (showReadsTrack && readsCanvasEl) {
      paintReadsTrack({
        canvas: readsCanvasEl,
        layout,
        cssW,
        viewStart,
        viewEnd,
        visibleBp,
        packed: pack.packed,
        hiddenCount: pack.hiddenCount,
        localBuffer,
        contig,
        isFetchingAlign,
        readsTruncated,
        readsTotal,
        selectedRead,
        colorReadsBy,
        colorBases,
        selectionStart,
        selectionEnd,
      });
    }
  }

  function isSequencePath(path: string) {
    return /\.(fasta|fa|fna|ffn|frn|fastq|fq)(\.gz)?$/i.test(path);
  }
  function isAnnotationPath(path: string) {
    return /\.(gff|gff3|bed)(\.gz)?$/i.test(path);
  }
  function isAlignmentPath(path: string) {
    return /\.(bam|cram)$/i.test(path);
  }

  async function openAnyPath(path: string) {
    if (isSequencePath(path)) await openSequencePath(path);
    else if (isAnnotationPath(path)) await openAnnotationPath(path);
    else if (isAlignmentPath(path)) await openAlignmentPath(path);
    else onLog(`Unsupported for View: ${path}`, "warn");
  }

  async function openSequencePath(path: string) {
    isLoading = true;
    errorMessage = "";
    slice = null;
    localBuffer = null;
    selectionStart = null;
    selectionEnd = null;
    statusMessage = `Opening ${path}…`;
    try {
      const doc = await viewOpenDocument(path);
      seqDoc = doc;
      contig = doc.contigs[0]?.name ?? contig;
      viewStart = 0;
      const firstLen = doc.contigs[0]?.length ?? 0;
      visibleBp = defaultVisibleBp(firstLen);
      reverseComplement = false;
      onLog(
        `Opened ${doc.format.toUpperCase()} — ${doc.contigs.length} contig(s), ${doc.totalBases.toLocaleString()} bases${doc.fullyCached ? " (cached)" : ""}.`,
      );
      statusMessage = `${doc.contigs.length} contig(s) · ${doc.totalBases.toLocaleString()} bases`;
      clampWindow();
      void prefetchAroundViewport(true);
      void refreshFeatures();
    } catch (error) {
      errorMessage = String(error);
      statusMessage = "Failed to open sequence.";
      onLog(`View open failed: ${String(error)}`, "error");
    } finally {
      isLoading = false;
    }
  }

  async function openAnnotationPath(path: string) {
    isLoading = true;
    errorMessage = "";
    statusMessage = `Opening annotations ${path}…`;
    try {
      const doc = await viewOpenAnnotation(path);
      annDocs = [...annDocs.filter((d) => d.path !== doc.path), doc];
      const label = doc.path.split(/[\\/]/).pop() ?? doc.path;
      annTrackStates = [
        ...annTrackStates.filter((t) => t.path !== doc.path),
        {
          path: doc.path,
          label,
          visible: true,
          features: [],
          truncated: false,
          totalInRange: 0,
        },
      ];
      if (!contig && doc.contigs.length > 0) {
        contig = doc.contigs[0]!;
        const span = doc.contigSpans?.find((s) => s.name === contig);
        const len = span?.length ?? 200;
        viewStart = 0;
        visibleBp = defaultVisibleBp(len);
      }
      onLog(
        `Opened ${doc.format.toUpperCase()} — ${doc.featureCount.toLocaleString()} feature(s).`,
      );
      statusMessage = `${annDocs.length} annotation file(s)`;
      clampWindow();
      void refreshFeatures();
    } catch (error) {
      errorMessage = String(error);
      onLog(`Annotation open failed: ${String(error)}`, "error");
    } finally {
      isLoading = false;
    }
  }

  async function openAlignmentPath(path: string) {
    const isCram = /\.cram$/i.test(path);
    if (isCram && !referencePath.trim()) {
      pendingCramPath = path;
      errorMessage =
        "CRAM requires a reference FASTA. Choose one below or right-click FASTA → Set as reference.";
      statusMessage = "Reference FASTA required for CRAM.";
      onLog("Cannot open CRAM without a reference FASTA.", "error");
      return;
    }
    isLoading = true;
    errorMessage = "";
    statusMessage = `Opening alignment ${path}…`;
    try {
      const doc = await viewOpenAlignment({
        path,
        referencePath: isCram ? referencePath.trim() : null,
      });
      alignDoc = doc;
      pendingCramPath = null;
      readCache = null;
      coverageCache = null;
      if (!contig && doc.contigs.length > 0) {
        contig = doc.contigs[0]!.name;
        const len = doc.contigs[0]!.length;
        viewStart = 0;
        visibleBp = defaultVisibleBp(len);
      }
      onLog(
        `Opened ${doc.format.toUpperCase()} — ${doc.contigs.length} contig(s)${doc.indexed ? ", indexed" : ""}.`,
      );
      statusMessage = `Alignment ${doc.format.toUpperCase()}`;
      clampWindow();
      void refreshAlignmentTracks();
    } catch (error) {
      if (isCram) pendingCramPath = path;
      errorMessage = String(error);
      onLog(`Alignment open failed: ${String(error)}`, "error");
    } finally {
      isLoading = false;
    }
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
      if (pendingCramPath) void openAlignmentPath(pendingCramPath);
    }
  }

  async function openSelected() {
    const seq = selectedSequencePath;
    const anns = selectedAnnotationPaths;
    const aln = selectedAlignmentPath;
    if (!seq && anns.length === 0 && !aln) {
      onLog("Select FASTA/FASTQ, GFF/BED, and/or BAM/CRAM first.", "error");
      return;
    }
    if (seq) await openSequencePath(seq);
    for (const path of anns) await openAnnotationPath(path);
    if (aln) await openAlignmentPath(aln);
  }

  function clearAnnotations() {
    annDocs = [];
    annTrackStates = [];
    selectedFeature = null;
  }

  function setAnnTrackVisible(path: string, visible: boolean) {
    annTrackStates = annTrackStates.map((t) =>
      t.path === path ? { ...t, visible } : t,
    );
  }

  function clearAlignment() {
    alignDoc = null;
    pendingCramPath = null;
    coverageBins = [];
    coverageMax = 0;
    alignmentReads = [];
    readsTruncated = false;
    readsTotal = 0;
    readCache = null;
    coverageCache = null;
    selectedRead = null;
    selectedCoverage = null;
  }

  function sliceFromLocalBuffer(): SequenceSlice | null {
    if (!localBuffer || localBuffer.contig !== contig) return null;
    if (viewStart < localBuffer.start || viewEnd > localBuffer.end) return null;
    const s = viewStart - localBuffer.start;
    const e = viewEnd - localBuffer.start;
    return {
      contig,
      start: viewStart,
      end: viewEnd,
      sequence: localBuffer.sequence.slice(s, e),
      contigLength,
      reverseComplemented: reverseComplement,
    };
  }

  function refreshVisibleSlice() {
    if (!seqDoc || !contig || contigLength <= 0) {
      slice = null;
      return;
    }
    clampWindow();
    if (visibleBp > MAX_SEQUENCE_WINDOW_BP) {
      slice = null;
      return;
    }
    const local = sliceFromLocalBuffer();
    if (local) {
      slice = local;
      maybeSchedulePrefetch();
      return;
    }
    void fetchExpandedWindow();
  }

  function maybeSchedulePrefetch() {
    if (!localBuffer) return;
    const margin = Math.min(
      SEQ_PREFETCH_PAD_BP,
      Math.floor((localBuffer.end - localBuffer.start) * 0.15),
    );
    const nearLeft = viewStart - localBuffer.start < margin;
    const nearRight = localBuffer.end - viewEnd < margin;
    if (nearLeft || nearRight) {
      if (prefetchTimer) clearTimeout(prefetchTimer);
      prefetchTimer = setTimeout(() => void prefetchAroundViewport(false), 60);
    }
  }

  async function prefetchAroundViewport(force: boolean) {
    if (!seqDoc || !contig || contigLength <= 0) return;
    if (visibleBp > MAX_SEQUENCE_WINDOW_BP) return;
    // Prefer a modest chunk around the viewport so first paint is fast.
    // Full-contig loads only for small contigs (≤ chunk size).
    if (contigLength > 0 && contigLength <= SEQ_PREFETCH_CHUNK_BP) {
      await loadBuffer(0, contigLength, force);
      return;
    }
    let desiredStart = Math.max(0, viewStart - SEQ_PREFETCH_PAD_BP);
    let desiredEnd = Math.min(contigLength, viewEnd + SEQ_PREFETCH_PAD_BP);
    if (desiredEnd - desiredStart < SEQ_PREFETCH_CHUNK_BP) {
      desiredEnd = Math.min(contigLength, desiredStart + SEQ_PREFETCH_CHUNK_BP);
      desiredStart = Math.max(0, desiredEnd - SEQ_PREFETCH_CHUNK_BP);
    }
    if (desiredEnd - desiredStart > MAX_SEQUENCE_WINDOW_BP) {
      const center = viewStart + visibleBp / 2;
      const half = Math.floor(MAX_SEQUENCE_WINDOW_BP / 2);
      desiredStart = Math.max(0, Math.floor(center - half));
      desiredEnd = Math.min(contigLength, desiredStart + MAX_SEQUENCE_WINDOW_BP);
    }
    await loadBuffer(desiredStart, desiredEnd, force);
  }

  async function fetchExpandedWindow() {
    // Viewport-first: never block first paint on a multi-megabase full-contig pull.
    if (contigLength > 0 && contigLength <= SEQ_PREFETCH_CHUNK_BP) {
      await loadBuffer(0, contigLength, true);
      return;
    }
    const pad = Math.max(SEQ_PREFETCH_PAD_BP, Math.floor(visibleBp * 2));
    let start = Math.max(0, viewStart - pad);
    let end = Math.min(contigLength, viewEnd + pad);
    const target = Math.min(SEQ_PREFETCH_CHUNK_BP, contigLength || SEQ_PREFETCH_CHUNK_BP);
    if (end - start < target) {
      end = Math.min(contigLength, start + target);
      start = Math.max(0, end - target);
    }
    if (end - start > MAX_SEQUENCE_WINDOW_BP) end = start + MAX_SEQUENCE_WINDOW_BP;
    await loadBuffer(start, end, true);
  }

  async function loadBuffer(start: number, end: number, updateVisible: boolean) {
    if (!seqDoc || !contig || end <= start) return;
    const token = ++fetchToken;
    isFetchingSlice = true;
    try {
      const next = await viewGetSequenceWindow({
        path: seqDoc.path,
        contig,
        start,
        end,
        reverseComplement,
      });
      if (token !== fetchToken) return;
      localBuffer = {
        contig,
        start: next.start,
        end: next.end,
        sequence: next.sequence,
        reverseComplement,
      };
      if (updateVisible) {
        slice = sliceFromLocalBuffer() ?? {
          contig: next.contig,
          start: viewStart,
          end: viewEnd,
          sequence: next.sequence.slice(
            Math.max(0, viewStart - next.start),
            Math.max(0, viewEnd - next.start),
          ),
          contigLength: next.contigLength,
          reverseComplemented: next.reverseComplemented,
        };
      }
    } catch (error) {
      if (token !== fetchToken) return;
      if (updateVisible) {
        slice = null;
        onLog(`Sequence fetch failed: ${String(error)}`, "warn");
      }
    } finally {
      if (token === fetchToken) isFetchingSlice = false;
    }
  }

  async function refreshFeatures() {
    if (annDocs.length === 0 || !contig || contigLength <= 0) {
      annTrackStates = annTrackStates.map((t) => ({
        ...t,
        features: [],
        truncated: false,
        totalInRange: 0,
      }));
      return;
    }
    clampWindow();
    const pad = Math.max(visibleBp, 1000);
    const start = Math.max(0, viewStart - pad);
    const end = Math.min(contigLength, viewEnd + pad);
    const token = ++featureFetchToken;
    isFetchingFeatures = true;
    try {
      const windows = await Promise.all(
        annDocs.map((doc) =>
          viewGetFeaturesInRange({
            path: doc.path,
            contig,
            start,
            end,
          }),
        ),
      );
      if (token !== featureFetchToken) return;
      const nextTracks: AnnTrackState[] = annDocs.map((doc, i) => {
        const existing = annTrackStates.find((t) => t.path === doc.path);
        const label = existing?.label ?? doc.path.split(/[\\/]/).pop() ?? doc.path;
        const window = windows[i]!;
        const feats = [...window.features].sort(
          (a, b) => a.start - b.start || b.end - a.end,
        );
        return {
          path: doc.path,
          label,
          visible: existing?.visible ?? true,
          features: feats,
          truncated: window.truncated || window.totalInRange > feats.length,
          totalInRange: window.totalInRange,
        };
      });
      annTrackStates = nextTracks;
    } catch (error) {
      if (token !== featureFetchToken) return;
      onLog(`Feature fetch failed: ${String(error)}`, "warn");
    } finally {
      if (token === featureFetchToken) isFetchingFeatures = false;
    }
  }

  function refreshAlignmentTracksDebounced() {
    if (alignDebounceTimer) clearTimeout(alignDebounceTimer);
    // Serve immediately from cache so pan/zoom never waits on IPC.
    applyAlignmentFromCaches();
    const filters = readFilterKey({
      hideSecondary,
      hideSupplementary,
      hideDuplicates,
      minMapq,
    });
    const binCount = Math.min(400, Math.max(40, Math.floor(plotWidth / 3)));
    const readsCovered =
      !!alignDoc &&
      cacheCoversWindow(readCache, alignDoc.path, contig, filters, viewStart, viewEnd);
    const covCovered =
      !!alignDoc &&
      coverageCacheCovers(
        coverageCache,
        alignDoc.path,
        contig,
        viewStart,
        viewEnd,
        binCount,
      );
    const fullyCovered = (!showReadPileup || readsCovered || visibleBp > READ_FETCH_MAX_BP) &&
      (!showCoverage || covCovered);
    const delay = fullyCovered
      ? isPanning || isOverviewDragging
        ? FETCH_DEBOUNCE_PAN_MS
        : FETCH_DEBOUNCE_IDLE_MS
      : isPanning || isOverviewDragging
        ? FETCH_DEBOUNCE_PAN_MS
        : visibleBp <= 80_000
          ? 40
          : 90;
    alignDebounceTimer = setTimeout(() => void refreshAlignmentTracks(), delay);
  }

  /** Instantly slice cached coverage/reads into the viewport (no IPC). */
  function applyAlignmentFromCaches() {
    if (!alignDoc || !contig) return;
    const filters = readFilterKey({
      hideSecondary,
      hideSupplementary,
      hideDuplicates,
      minMapq,
    });
    const binCount = Math.min(400, Math.max(40, Math.floor(plotWidth / 3)));

    if (
      showCoverage &&
      coverageCacheCovers(
        coverageCache,
        alignDoc.path,
        contig,
        viewStart,
        viewEnd,
        binCount,
      ) &&
      coverageCache
    ) {
      coverageBins = coverageCache.bins.filter(
        (b) => b.end > viewStart && b.start < viewEnd,
      );
      coverageMax = coverageCache.maxDepth;
    }

    if (
      showReadPileup &&
      visibleBp <= READ_FETCH_MAX_BP &&
      cacheCoversWindow(readCache, alignDoc.path, contig, filters, viewStart, viewEnd) &&
      readCache
    ) {
      alignmentReads = readsOverlapWindow(readCache.reads, viewStart, viewEnd);
      readsTruncated = readCache.truncated;
      readsTotal = readCache.totalInRange;
    }
  }

  async function refreshAlignmentTracks() {
    if (!alignDoc || !contig || contigLength <= 0) {
      coverageBins = [];
      coverageMax = 0;
      alignmentReads = [];
      readsTotal = 0;
      return;
    }
    clampWindow();
    const token = ++alignFetchToken;
    const ref =
      alignDoc.requiresReference && referencePath.trim() ? referencePath.trim() : null;
    const filters = readFilterKey({
      hideSecondary,
      hideSupplementary,
      hideDuplicates,
      minMapq,
    });
    const binCount = Math.min(400, Math.max(40, Math.floor(plotWidth / 3)));

    try {
      const wantReads = showReadPileup && visibleBp <= READ_FETCH_MAX_BP;
      const includeSequences = visibleBp <= BASE_LETTERS_MAX_BP;
      const readsHit =
        wantReads &&
        cacheCoversWindow(readCache, alignDoc.path, contig, filters, viewStart, viewEnd);
      const covHit =
        showCoverage &&
        coverageCacheCovers(
          coverageCache,
          alignDoc.path,
          contig,
          viewStart,
          viewEnd,
          binCount,
        );

      // Apply any existing cache first so UI stays live while network runs.
      applyAlignmentFromCaches();

      const needsNetwork = (showCoverage && !covHit) || (wantReads && !readsHit);
      if (needsNetwork) isFetchingAlign = true;

      let covPromise: ReturnType<typeof viewGetCoverageBins> | null = null;
      let covFetchStart = 0;
      let covFetchEnd = 0;
      if (showCoverage && !covHit) {
        ({ start: covFetchStart, end: covFetchEnd } = computeCoverageFetchWindow(
          viewStart,
          viewEnd,
          contigLength,
          visibleBp,
        ));
        covPromise = viewGetCoverageBins({
          path: alignDoc.path,
          contig,
          start: covFetchStart,
          end: covFetchEnd,
          binCount,
          referencePath: ref,
        });
      }

      let readPromise: ReturnType<typeof viewGetReadsInRange> | null = null;
      let fetchStart = 0;
      let fetchEnd = 0;
      if (wantReads && !readsHit) {
        ({ start: fetchStart, end: fetchEnd } = computeReadFetchWindow(
          viewStart,
          viewEnd,
          contigLength,
          visibleBp,
        ));
        readPromise = viewGetReadsInRange({
          path: alignDoc.path,
          contig,
          start: fetchStart,
          end: fetchEnd,
          referencePath: ref,
          includeSecondary: !hideSecondary,
          includeSupplementary: !hideSupplementary,
          includeDuplicates: !hideDuplicates,
          minMapq,
          includeSequences,
        });
      }

      const [cov, reads] = await Promise.all([covPromise, readPromise]);
      if (token !== alignFetchToken) return;

      if (showCoverage) {
        if (cov) {
          coverageCache = {
            path: alignDoc.path,
            contig,
            start: covFetchStart,
            end: covFetchEnd,
            binCount,
            bins: cov.bins,
            maxDepth: cov.maxDepth,
          };
          coverageBins = cov.bins.filter((b) => b.end > viewStart && b.start < viewEnd);
          coverageMax = cov.maxDepth;
          if (selectedCoverage) {
            const still = cov.bins.find(
              (b) => b.start === selectedCoverage!.start && b.end === selectedCoverage!.end,
            );
            selectedCoverage = still ?? null;
          }
        } else if (covHit && coverageCache) {
          coverageBins = coverageCache.bins.filter(
            (b) => b.end > viewStart && b.start < viewEnd,
          );
          coverageMax = coverageCache.maxDepth;
          if (coverageNearEdge(coverageCache, viewStart, viewEnd, visibleBp)) {
            void expandCoverageCache(ref, binCount, token);
          }
        }
      } else {
        coverageBins = [];
        coverageMax = 0;
        selectedCoverage = null;
      }

      if (wantReads) {
        if (readsHit && readCache) {
          alignmentReads = readsOverlapWindow(readCache.reads, viewStart, viewEnd);
          readsTruncated = readCache.truncated;
          readsTotal = readCache.totalInRange;
          if (cacheNearEdge(readCache, viewStart, viewEnd, visibleBp)) {
            void expandReadCache(ref, filters, token);
          }
        } else if (reads) {
          readCache = {
            path: alignDoc.path,
            contig,
            start: fetchStart,
            end: fetchEnd,
            filterKey: filters,
            reads: reads.reads,
            totalInRange: reads.totalInRange,
            truncated: reads.truncated,
          };
          alignmentReads = readsOverlapWindow(reads.reads, viewStart, viewEnd);
          readsTruncated = reads.truncated;
          readsTotal = reads.totalInRange;
        }

        if (selectedRead) {
          const still = alignmentReads.find(
            (r) =>
              r.name === selectedRead!.name &&
              r.start === selectedRead!.start &&
              r.flags === selectedRead!.flags,
          );
          selectedRead = still ?? null;
        }
      } else {
        alignmentReads = [];
        readsTruncated = false;
        readsTotal = 0;
        if (visibleBp > READ_FETCH_MAX_BP) selectedRead = null;
      }
    } catch (error) {
      if (token !== alignFetchToken) return;
      onLog(`Alignment track failed: ${String(error)}`, "warn");
    } finally {
      if (token === alignFetchToken) isFetchingAlign = false;
    }
  }

  async function expandCoverageCache(
    ref: string | null,
    binCount: number,
    token: number,
  ) {
    if (!alignDoc || !contig || contigLength <= 0) return;
    const { start: fetchStart, end: fetchEnd } = computeCoverageFetchWindow(
      viewStart,
      viewEnd,
      contigLength,
      visibleBp,
    );
    if (
      coverageCache &&
      coverageCache.path === alignDoc.path &&
      coverageCache.contig === contig &&
      fetchStart >= coverageCache.start &&
      fetchEnd <= coverageCache.end
    ) {
      return;
    }
    try {
      const cov = await viewGetCoverageBins({
        path: alignDoc.path,
        contig,
        start: fetchStart,
        end: fetchEnd,
        binCount,
        referencePath: ref,
      });
      if (token !== alignFetchToken) return;
      coverageCache = {
        path: alignDoc.path,
        contig,
        start: fetchStart,
        end: fetchEnd,
        binCount,
        bins: cov.bins,
        maxDepth: cov.maxDepth,
      };
      coverageBins = cov.bins.filter((b) => b.end > viewStart && b.start < viewEnd);
      coverageMax = cov.maxDepth;
    } catch {
      /* non-fatal */
    }
  }

  async function expandReadCache(ref: string | null, filters: string, token: number) {
    if (!alignDoc || !contig || contigLength <= 0) return;
    const { start: fetchStart, end: fetchEnd } = computeReadFetchWindow(
      viewStart,
      viewEnd,
      contigLength,
      visibleBp,
    );
    if (
      readCache &&
      readCache.path === alignDoc.path &&
      readCache.contig === contig &&
      readCache.filterKey === filters &&
      fetchStart >= readCache.start &&
      fetchEnd <= readCache.end
    ) {
      return;
    }
    try {
      const reads = await viewGetReadsInRange({
        path: alignDoc.path,
        contig,
        start: fetchStart,
        end: fetchEnd,
        referencePath: ref,
        includeSecondary: !hideSecondary,
        includeSupplementary: !hideSupplementary,
        includeDuplicates: !hideDuplicates,
        minMapq,
        includeSequences: visibleBp <= BASE_LETTERS_MAX_BP,
      });
      if (token !== alignFetchToken) return;
      readCache = {
        path: alignDoc.path,
        contig,
        start: fetchStart,
        end: fetchEnd,
        filterKey: filters,
        reads: reads.reads,
        totalInRange: reads.totalInRange,
        truncated: reads.truncated,
      };
      alignmentReads = readsOverlapWindow(reads.reads, viewStart, viewEnd);
      readsTruncated = reads.truncated;
      readsTotal = reads.totalInRange;
    } catch {
      /* non-fatal */
    }
  }

  function selectRead(read: AlignmentRead) {
    selectedRead = read;
    selectedFeature = null;
    selectedCoverage = null;
    if (read.end <= viewStart || read.start >= viewEnd) {
      const len = Math.max(read.end - read.start, MIN_VISIBLE_BP);
      visibleBp = Math.min(contigLength || read.end, Math.max(len * 4, 120));
      viewStart = Math.max(0, read.start - Math.floor(visibleBp / 4));
      clampWindow();
    }
  }

  function zoomToRead(read: AlignmentRead) {
    selectedRead = read;
    selectedFeature = null;
    selectedCoverage = null;
    const len = Math.max(read.end - read.start, MIN_VISIBLE_BP);
    visibleBp = Math.min(contigLength || read.end, Math.max(len * 4, 80));
    viewStart = Math.max(0, read.start - Math.floor(visibleBp / 4));
    clampWindow();
  }

  function zoomToFeature(feature: AnnotationFeature) {
    selectedFeature = feature;
    selectedRead = null;
    selectedCoverage = null;
    const len = Math.max(feature.end - feature.start, MIN_VISIBLE_BP);
    visibleBp = Math.min(contigLength || feature.end, Math.max(len * 3, 60));
    viewStart = Math.max(0, feature.start - Math.floor(visibleBp / 3));
    clampWindow();
  }

  function zoomFitContig() {
    if (contigLength <= 0) return;
    viewStart = 0;
    visibleBp = contigLength;
    clampWindow();
  }

  function zoomToSelection() {
    if (selectionStart == null || selectionEnd == null || selectionEnd <= selectionStart) {
      onLog("Select a region first (drag on the tracks).", "warn");
      return;
    }
    visibleBp = Math.min(contigLength, Math.max(selectionEnd - selectionStart, MIN_VISIBLE_BP));
    viewStart = selectionStart;
    clampWindow();
  }

  function zoomInCenter() {
    zoomAtClientX(null, ZOOM_IN_FACTOR);
  }

  function zoomOutCenter() {
    zoomAtClientX(null, ZOOM_OUT_FACTOR);
  }

  function zoomAtClientX(clientX: number | null, factor: number) {
    if (contigLength <= 0) return;
    const el = canvasEl ?? readsCanvasEl;
    const anchor =
      clientX != null && el
        ? genomicFromClientX(clientX, el, viewStart, visibleBp, PAD_L, PAD_R)
        : viewStart + visibleBp / 2;
    const center = anchor ?? viewStart + visibleBp / 2;
    const t =
      visibleBp > 0 && anchor != null
        ? Math.max(0, Math.min(1, (anchor - viewStart) / visibleBp))
        : 0.5;
    visibleBp = Math.round(visibleBp * factor);
    clampWindow();
    viewStart = Math.round(center - t * visibleBp);
    clampWindow();
  }

  function applyLocusFromInput() {
    const parsed = parseLocus(locusInput, contig, contigLength);
    if (!parsed.ok) {
      onLog(`Locus: ${parsed.message}`, "warn");
      if (contig) locusInput = formatLocus(contig, viewStart, viewEnd);
      return;
    }
    if (parsed.contig !== contig) {
      const known = contigOptions.some((c) => c.name === parsed.contig);
      if (known) contig = parsed.contig;
      else {
        onLog(`Unknown contig: ${parsed.contig}`, "warn");
        return;
      }
    }
    visibleBp = Math.max(MIN_VISIBLE_BP, parsed.viewEnd - parsed.viewStart);
    viewStart = parsed.viewStart;
    clampWindow();
    locusInput = formatLocus(contig, viewStart, viewEnd);
  }

  function overviewCenterAt(clientX: number, el: HTMLElement) {
    if (contigLength <= 0) return;
    const layout = viewerLayout;
    const rect = el.getBoundingClientRect();
    const x = clientX - rect.left;
    const usable = Math.max(1, layout.usable);
    const t = Math.max(0, Math.min(1, (x - layout.padL) / usable));
    const center = Math.floor(t * contigLength);
    viewStart = Math.max(0, Math.min(contigLength - visibleBp, center - Math.floor(visibleBp / 2)));
    clampWindow();
  }

  function onViewerKeydown(event: KeyboardEvent) {
    if (!hasDocument || disabled) return;
    const target = event.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA") {
      return;
    }
    if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      zoomInCenter();
    } else if (event.key === "-" || event.key === "_") {
      event.preventDefault();
      zoomOutCenter();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      const step = Math.max(1, Math.floor(visibleBp * KEY_PAN_FRACTION));
      viewStart = Math.max(0, viewStart - step);
      clampWindow();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      const step = Math.max(1, Math.floor(visibleBp * KEY_PAN_FRACTION));
      viewStart = Math.min(Math.max(0, contigLength - visibleBp), viewStart + step);
      clampWindow();
    } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "c") {
      if (selectionStart != null && selectionEnd != null) {
        event.preventDefault();
        void copySelection();
      }
    }
  }

  function onContigChange(name: string) {
    contig = name;
    viewStart = 0;
    const len =
      seqDoc?.contigs.find((c) => c.name === name)?.length ??
      alignDoc?.contigs.find((c) => c.name === name)?.length ??
      0;
    visibleBp = Math.min(Math.max(MIN_VISIBLE_BP, Math.min(visibleBp, len || MIN_VISIBLE_BP)), len || MIN_VISIBLE_BP);
    selectionStart = null;
    selectionEnd = null;
    selectedFeature = null;
    selectedRead = null;
    localBuffer = null;
    readCache = null;
    coverageCache = null;
    clampWindow();
    if (seqDoc) void prefetchAroundViewport(true);
  }

  function coverageAtClient(clientX: number, clientY: number): CoverageBin | null {
    if (!canvasEl || !showCovTrack || coverageBins.length === 0 || visibleBp <= 0) return null;
    const rect = canvasEl.getBoundingClientRect();
    const y = clientY - rect.top;
    const layout = viewerLayout;
    if (!layout.cov || y < layout.cov.top || y > layout.cov.top + layout.cov.height) return null;
    const g = genomicFromClientX(clientX, canvasEl, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return null;
    for (const bin of coverageBins) {
      if (g >= bin.start && g < bin.end) return bin;
    }
    return null;
  }

  function featureAtClient(clientX: number, clientY: number): AnnotationFeature | null {
    if (!canvasEl || visibleBp <= 0) return null;
    const rect = canvasEl.getBoundingClientRect();
    const y = clientY - rect.top;
    const layout = viewerLayout;
    const g = genomicFromClientX(clientX, canvasEl, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return null;
    for (const band of layout.annTracks) {
      if (y < band.top || y > band.top + band.height) continue;
      const track = annTrackStates.find((t) => t.path === band.path);
      if (!track?.visible) continue;
      let best: AnnotationFeature | null = null;
      for (const f of track.features) {
        if (g >= f.start && g < f.end) {
          if (!best || f.end - f.start < best.end - best.start) best = f;
        }
      }
      if (best) return best;
    }
    return null;
  }

  function readAtClient(clientX: number, clientY: number): AlignmentRead | null {
    if (!readsCanvasEl || !showReadsTrack || readPack.packed.length === 0) return null;
    const rect = readsCanvasEl.getBoundingClientRect();
    const y = clientY - rect.top;
    const g = genomicFromClientX(clientX, readsCanvasEl, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return null;
    const headerH = 20;
    const lane = Math.floor((y - headerH - 3) / READ_LANE_H);
    if (lane < 0 || lane >= readPack.packed.length) return null;
    const item = readPack.packed[lane];
    if (!item) return null;
    if (g >= item.read.start && g < item.read.end) return item.read;
    return null;
  }

  function onCanvasPointerDown(event: PointerEvent) {
    if (!hasDocument) return;
    const target = event.currentTarget as HTMLCanvasElement;
    target.setPointerCapture(event.pointerId);
    wrapEl?.focus();

    if (target === canvasEl && hitOverview(event.clientY, target, viewerLayout)) {
      isOverviewDragging = true;
      overviewCenterAt(event.clientX, target);
      return;
    }

    if (event.button === 1) {
      isPanning = true;
      panOriginX = event.clientX;
      panOriginStart = viewStart;
      return;
    }

    if (target === readsCanvasEl) {
      const hitRead = readAtClient(event.clientX, event.clientY);
      if (hitRead && !event.shiftKey) {
        selectRead(hitRead);
        dragOrigin = null;
        return;
      }
    }

    if (!event.shiftKey) {
      const hitCov = coverageAtClient(event.clientX, event.clientY);
      if (hitCov) {
        selectedCoverage = hitCov;
        selectedRead = null;
        selectedFeature = null;
      }
      const hit = featureAtClient(event.clientX, event.clientY);
      if (hit) {
        selectedFeature = hit;
        selectedRead = null;
        selectedCoverage = null;
      }
      isPanning = true;
      panOriginX = event.clientX;
      panOriginStart = viewStart;
      return;
    }

    const g = genomicFromClientX(event.clientX, target, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return;
    selectedFeature = null;
    selectedRead = null;
    selectedCoverage = null;
    dragOrigin = g;
    selectionStart = g;
    selectionEnd = g + 1;
  }

  function onCanvasPointerMove(event: PointerEvent) {
    const target = event.currentTarget as HTMLCanvasElement;
    if (isOverviewDragging && target === canvasEl) {
      overviewCenterAt(event.clientX, target);
      return;
    }
    if (isPanning) {
      const layout = viewerLayout;
      const usable = Math.max(1, layout.usable);
      const dx = event.clientX - panOriginX;
      viewStart = panOriginStart + Math.round((-dx / usable) * visibleBp);
      clampWindow();
      return;
    }
    if (dragOrigin == null) return;
    const g = genomicFromClientX(event.clientX, target, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return;
    selectionStart = Math.min(dragOrigin, g);
    selectionEnd = Math.max(dragOrigin, g) + 1;
  }

  function onCanvasPointerUp(event: PointerEvent) {
    const target = event.currentTarget as HTMLCanvasElement;
    if (target.hasPointerCapture(event.pointerId)) target.releasePointerCapture(event.pointerId);
    const wasPanning = isPanning || isOverviewDragging;
    isPanning = false;
    isOverviewDragging = false;
    dragOrigin = null;
    if (wasPanning) refreshAlignmentTracksDebounced();
  }

  function onCanvasDoubleClick(event: MouseEvent) {
    const hitRead = readAtClient(event.clientX, event.clientY);
    if (hitRead) {
      zoomToRead(hitRead);
      return;
    }
    const hit = featureAtClient(event.clientX, event.clientY);
    if (hit) {
      zoomToFeature(hit);
      return;
    }
    zoomAtClientX(event.clientX, ZOOM_IN_FACTOR);
  }

  function onCanvasWheel(event: WheelEvent) {
    event.preventDefault();
    if (event.shiftKey) {
      const step = Math.max(1, Math.floor(visibleBp * 0.08));
      viewStart += event.deltaY > 0 ? step : -step;
      clampWindow();
      return;
    }
    zoomAtClientX(event.clientX, event.deltaY > 0 ? ZOOM_OUT_FACTOR : ZOOM_IN_FACTOR);
  }

  async function copySelection() {
    if (selectionStart == null || selectionEnd == null || !seqDoc || !contig) {
      onLog("Select a region on the tracks first.", "warn");
      return;
    }
    if (selectionEnd - selectionStart > MAX_SEQUENCE_WINDOW_BP) {
      onLog(`Selection too large (max ${MAX_SEQUENCE_WINDOW_BP.toLocaleString()} bp).`, "warn");
      return;
    }
    try {
      const data = await viewGetSequenceWindow({
        path: seqDoc.path,
        contig,
        start: selectionStart,
        end: selectionEnd,
        reverseComplement,
      });
      await navigator.clipboard.writeText(data.sequence);
      onLog(`Copied ${data.sequence.length.toLocaleString()} bases (${contig}:${selectionStart + 1}–${selectionEnd}).`);
    } catch (error) {
      onLog(`Copy failed: ${String(error)}`, "error");
    }
  }

  function toggleMenu(id: "display" | "filters") {
    openMenu = openMenu === id ? null : id;
  }
  function closeMenus() {
    openMenu = null;
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="pane-body" onclick={closeMenus}>
  <div class="toolbar" onclick={(e) => e.stopPropagation()}>
    <button
      class="primary compact"
      onclick={openSelected}
      disabled={disabled ||
        isLoading ||
        (!selectedSequencePath && selectedAnnotationPaths.length === 0 && !selectedAlignmentPath) ||
        (selectedIsCram && !cramReferenceReady)}
      title={selectedIsCram && !cramReferenceReady ? "Set a reference FASTA before opening CRAM" : undefined}
    >
      {isLoading ? "Opening…" : "Open"}
    </button>

    {#if hasDocument}
      {#if contigOptions.length > 1}
        <select
          class="compact-select"
          value={contig}
          onchange={(e) => onContigChange((e.currentTarget as HTMLSelectElement).value)}
          disabled={disabled || isLoading}
          title="Contig"
        >
          {#each contigOptions as c}
            <option value={c.name}>{c.name}</option>
          {/each}
        </select>
      {/if}

      <input
        class="locus-input"
        type="text"
        bind:value={locusInput}
        disabled={disabled}
        title="IGV locus (e.g. chr1:1,000-2,000)"
        onfocus={() => (locusEditing = true)}
        onblur={() => {
          locusEditing = false;
          applyLocusFromInput();
        }}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            (e.currentTarget as HTMLInputElement).blur();
          }
        }}
      />

      <div class="zoom-cluster">
        <button type="button" class="ghost compact icon-btn" title="Zoom out (−)" onclick={zoomOutCenter} disabled={disabled}>−</button>
        <button type="button" class="ghost compact icon-btn" title="Zoom in (+)" onclick={zoomInCenter} disabled={disabled}>+</button>
        <button type="button" class="ghost compact" title="Whole contig" onclick={zoomFitContig} disabled={disabled || contigLength <= 0}>All</button>
        <button
          type="button"
          class="ghost compact"
          title="Zoom to selection (Shift+drag)"
          onclick={zoomToSelection}
          disabled={disabled || selectionStart == null}
        >Sel</button>
        <button
          type="button"
          class="ghost compact"
          title="Copy selection (Ctrl+C)"
          onclick={() => void copySelection()}
          disabled={disabled || !seqDoc || selectionStart == null}
        >Copy</button>
      </div>

      <div class="menu-pop">
        <button type="button" class="ghost compact" class:active={openMenu === "display"} onclick={() => toggleMenu("display")}>
          Display ▾
        </button>
        {#if openMenu === "display"}
          <div class="pop-menu">
            {#if seqDoc}
              <label class="pop-check"><input type="checkbox" bind:checked={reverseComplement} /> Rev-comp reference</label>
              <label class="pop-check"><input type="checkbox" bind:checked={colorBases} /> Color bases</label>
            {/if}
            {#if alignDoc}
              <label class="pop-check"><input type="checkbox" bind:checked={colorMismatches} /> Color mismatches in coverage</label>
            {/if}
            {#if annDocs.length > 0}
              <div class="pop-sep"></div>
              <button type="button" class="pop-item" onclick={() => { clearAnnotations(); closeMenus(); }}>Clear annotations</button>
            {/if}
            {#if alignDoc}
              <button type="button" class="pop-item" onclick={() => { clearAlignment(); closeMenus(); }}>Clear alignment</button>
            {/if}
          </div>
        {/if}
      </div>

      {#if alignDoc}
        <div class="menu-pop">
          <button type="button" class="ghost compact" class:active={openMenu === "filters"} onclick={() => toggleMenu("filters")}>
            Filters ▾
          </button>
          {#if openMenu === "filters"}
            <div class="pop-menu filters-menu">
              <label class="pop-check"><input type="checkbox" bind:checked={hideSecondary} /> Hide secondary</label>
              <label class="pop-check"><input type="checkbox" bind:checked={hideSupplementary} /> Hide supplementary</label>
              <label class="pop-check"><input type="checkbox" bind:checked={hideDuplicates} /> Hide duplicates</label>
              <label class="pop-field">
                <span>Min MAPQ</span>
                <input type="number" min="0" max="60" bind:value={minMapq} />
              </label>
              <label class="pop-field">
                <span>Color reads</span>
                <select bind:value={colorReadsBy}>
                  <option value="strand">Strand</option>
                  <option value="none">Gray</option>
                  <option value="mapq">MAPQ</option>
                  <option value="pair">Pair</option>
                </select>
              </label>
            </div>
          {/if}
        </div>
      {/if}
    {/if}

    {#if seqDoc}
      <span class="file-label" title={seqDoc.path}>{seqDoc.path.split(/[\\/]/).pop()}</span>
    {/if}
    {#if alignDoc}
      <span class="file-label" title={alignDoc.path}>{alignDoc.path.split(/[\\/]/).pop()}</span>
    {/if}
  </div>

  {#if showCramReferenceField}
    <div class="field cram-ref" onclick={(e) => e.stopPropagation()}>
      <div class="label-with-help">
        <span>CRAM reference</span>
        <InfoLink section="cram-and-reference" label="CRAM reference — manual" />
      </div>
      <div class="row">
        <button class="ghost compact" onclick={browseReferenceFasta} disabled={disabled || isLoading}>Choose</button>
        <input bind:value={referencePath} placeholder="Reference FASTA path" disabled={disabled || isLoading} />
      </div>
    </div>
  {/if}

  {#if hasDocument}
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="viewer"
      bind:this={wrapEl}
      tabindex="0"
      onkeydown={onViewerKeydown}
    >
      <div class="viewer-main">
        <div class="track-gutter" style:width="{GUTTER_WIDTH}px">
          {#each viewerLayout.bands as band (band.id)}
            {#if band.id !== "reads"}
              <div class="gutter-row" style:height="{band.height}px" title={band.label}>
                {#if band.id === "seq"}
                  <label class="gutter-check">
                    <input type="checkbox" bind:checked={showSeqTrackVisible} disabled={!seqDoc} />
                  </label>
                {:else if band.id === "cov"}
                  <label class="gutter-check">
                    <input type="checkbox" bind:checked={showCoverage} disabled={!alignDoc} />
                  </label>
                {:else if band.id.startsWith("ann:")}
                  {@const annMeta = viewerLayout.annTracks.find((t) => t.top === band.top)}
                  <label class="gutter-check">
                    <input
                      type="checkbox"
                      checked={annTrackStates.find((t) => t.path === annMeta?.path)?.visible ?? true}
                      onchange={(e) =>
                        annMeta && setAnnTrackVisible(annMeta.path, (e.currentTarget as HTMLInputElement).checked)}
                    />
                  </label>
                {:else}
                  <span class="gutter-spacer"></span>
                {/if}
                <span class="gutter-label">{band.label}</span>
              </div>
            {/if}
          {/each}
          {#if showReadsTrack && viewerLayout.reads}
            <div class="gutter-row gutter-reads">
              <label class="gutter-check">
                <input type="checkbox" bind:checked={showReadPileup} disabled={!alignDoc} />
              </label>
              <span class="gutter-label">Alignments</span>
            </div>
          {/if}
        </div>
        <div class="track-plots">
          <div class="fixed-tracks">
            <canvas
              bind:this={canvasEl}
              class="view-canvas"
              class:panning={isPanning || isOverviewDragging}
              onpointerdown={onCanvasPointerDown}
              onpointermove={onCanvasPointerMove}
              onpointerup={onCanvasPointerUp}
              onpointercancel={onCanvasPointerUp}
              ondblclick={onCanvasDoubleClick}
              onwheel={onCanvasWheel}
            ></canvas>
          </div>
          {#if showReadsTrack}
            <div class="reads-scroll">
              <canvas
                bind:this={readsCanvasEl}
                class="view-canvas"
                class:panning={isPanning}
                onpointerdown={onCanvasPointerDown}
                onpointermove={onCanvasPointerMove}
                onpointerup={onCanvasPointerUp}
                onpointercancel={onCanvasPointerUp}
                ondblclick={onCanvasDoubleClick}
                onwheel={onCanvasWheel}
              ></canvas>
            </div>
          {/if}
        </div>
      </div>
      <div class="viewer-footer">
        {#if selectedRead}
          <span class="footer-detail">
            <strong>{selectedRead.name}</strong>
            · {contig}:{selectedRead.start + 1}–{selectedRead.end}
            · MAPQ {selectedRead.mapq}
            · {selectedRead.strand}
            · {flagSummary(selectedRead)}
            · CIGAR {selectedRead.cigar}
            · Q̄ {meanQuality(selectedRead)}
          </span>
        {:else if selectedFeature}
          <span class="footer-detail">
            <strong>{selectedFeature.name || selectedFeature.featureType}</strong>
            · {selectedFeature.contig}:{selectedFeature.start + 1}–{selectedFeature.end}
            · {selectedFeature.featureType}
            · strand {selectedFeature.strand}
          </span>
        {:else if selectedCoverage}
          <span class="footer-detail">
            Coverage {selectedCoverage.start + 1}–{selectedCoverage.end}
            · depth {selectedCoverage.depth.toFixed(1)}×
            · max {coverageMax.toFixed(1)}×
          </span>
        {:else if contigLength > 0}
          <span class="footer-hint">
            Drag to pan · Shift+drag to select · wheel zoom · overview scrub · +/− keys
            {#if readPack.hiddenCount > 0} · +{readPack.hiddenCount} reads hidden (zoom in){/if}
          </span>
        {/if}
      </div>
    </div>
  {/if}

  {#if errorMessage}
    <div class="status bad">{errorMessage}</div>
  {:else}
    <p class="status-line">
      {statusMessage}{isFetchingSlice || isFetchingFeatures || isFetchingAlign ? " · fetching…" : ""}
    </p>
  {/if}

  {#if !hasDocument}
    <p class="subtle">
      Select <code>.fasta</code>, optional <code>.gff</code>/<code>.bed</code>, and/or indexed
      <code>.bam</code>/<code>.cram</code> → Open. CRAM needs a reference FASTA.
    </p>
  {/if}
</div>

<style>
  .pane-body {
    display: flex;
    flex-direction: column;
    min-height: 0;
    flex: 1;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 6px;
    flex-wrap: wrap;
  }

  .compact-select,
  .locus-input {
    padding: 4px 8px;
    border-radius: 4px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    font: inherit;
    font-size: 0.78rem;
  }

  .locus-input {
    flex: 1 1 180px;
    min-width: 140px;
    max-width: 320px;
    font-family: Arial, Helvetica, sans-serif;
  }

  .zoom-cluster {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .icon-btn {
    min-width: 28px;
    padding-inline: 6px;
    font-size: 0.95rem;
    line-height: 1;
  }

  .compact-select {
    max-width: 120px;
  }

  .menu-pop {
    position: relative;
  }

  .menu-pop > .ghost.active {
    border-color: var(--chip-active-border);
    color: var(--chip-active-text);
  }

  .pop-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 180px;
    padding: 6px;
    border-radius: 10px;
    border: 1px solid var(--dropdown-border);
    background: var(--dropdown-bg);
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.35);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .pop-item,
  .pop-check,
  .pop-field {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 6px 8px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-menu);
    font: inherit;
    font-size: 0.8rem;
    cursor: pointer;
  }

  .pop-item:hover:not(:disabled),
  .pop-check:hover {
    background: var(--dropdown-hover-bg);
    color: var(--menu-active-text);
  }

  .pop-item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .pop-field {
    flex-direction: column;
    align-items: stretch;
    cursor: default;
  }

  .pop-field span {
    font-size: 0.7rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .pop-field input,
  .pop-field select {
    padding: 5px 8px;
    border-radius: 8px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    font: inherit;
    font-size: 0.8rem;
  }

  .pop-sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--chip-border);
  }

  .filters-menu {
    min-width: 200px;
  }

  .file-label {
    font-size: 0.72rem;
    color: var(--text-muted);
    font-family: "JetBrains Mono", monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 140px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin-bottom: 6px;
  }

  .cram-ref {
    padding: 6px 8px;
    border-radius: 10px;
    border: 1px solid var(--chip-border);
    background: var(--chip-bg);
  }

  .label-with-help {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-menu);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .row {
    display: flex;
    gap: 6px;
    min-width: 0;
  }

  .row > .ghost {
    flex: 0 0 auto;
  }

  .row > input {
    flex: 1 1 auto;
    min-width: 0;
    padding: 6px 8px;
    border-radius: 8px;
    border: 1px solid var(--input-border);
    background: var(--input-bg);
    color: var(--text-primary);
    font: inherit;
    font-size: 0.8rem;
  }

  .ghost,
  .primary {
    cursor: pointer;
    border: none;
    padding: 6px 10px;
    border-radius: 8px;
    font-weight: 600;
    font-size: 0.8rem;
    font: inherit;
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

  .viewer {
    flex: 1 1 auto;
    min-height: 220px;
    display: flex;
    flex-direction: column;
    border-radius: 4px;
    border: 1px solid var(--view-track-border, var(--panel-border));
    overflow: hidden;
    background: var(--view-canvas-bg, #f7f7f7);
    outline: none;
  }

  .viewer:focus-visible {
    border-color: var(--chip-active-border);
  }

  .viewer-main {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: row;
  }

  .track-gutter {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--view-track-border, var(--panel-border));
    background: var(--view-gutter-bg, var(--chip-bg));
    overflow: hidden;
  }

  .gutter-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 4px;
    border-bottom: 1px solid var(--view-track-border, var(--panel-border));
    min-height: 0;
    flex-shrink: 0;
  }

  .gutter-row.gutter-reads {
    flex: 1 1 auto;
    min-height: 120px;
    align-items: flex-start;
    padding-top: 4px;
  }

  .gutter-check {
    display: flex;
    margin: 0;
    flex: 0 0 auto;
  }

  .gutter-check input {
    margin: 0;
    width: 12px;
    height: 12px;
  }

  .gutter-spacer {
    width: 12px;
    flex: 0 0 12px;
  }

  .gutter-label {
    font-size: 0.68rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.2;
  }

  .track-plots {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .fixed-tracks {
    flex: 0 0 auto;
  }

  .reads-scroll {
    flex: 1 1 auto;
    min-height: 120px;
    overflow-x: hidden;
    overflow-y: auto;
    border-top: 1px solid var(--view-track-border, var(--panel-border));
  }

  .view-canvas {
    display: block;
    width: 100%;
    cursor: default;
    touch-action: none;
  }

  .view-canvas.panning {
    cursor: grabbing;
  }

  .viewer-footer {
    flex: 0 0 auto;
    padding: 3px 8px 4px;
    font-size: 0.68rem;
    color: var(--text-muted);
    border-top: 1px solid var(--view-track-border, var(--panel-border));
    background: var(--view-gutter-bg, var(--chip-bg));
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .footer-detail strong {
    color: var(--text-primary);
    font-weight: 600;
  }

  .footer-hint {
    color: var(--text-faint);
  }

  .status {
    margin-top: 6px;
    padding: 6px 10px;
    border-radius: 8px;
    font-size: 0.8rem;
  }

  .status.bad {
    border: 1px solid var(--status-error-border);
    background: var(--status-error-bg);
    color: var(--status-error-text);
  }

  .status-line {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 0.74rem;
  }

  .subtle {
    margin: 6px 0 0;
    color: var(--text-muted);
    font-size: 0.76rem;
    line-height: 1.4;
  }

  .subtle code {
    font-family: "JetBrains Mono", monospace;
    font-size: 0.72rem;
    color: var(--code-color);
  }
</style>
