<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import InfoLink from "$lib/components/InfoLink.svelte";
  import {
    viewGetCoverageBins,
    viewGetFeaturesInRange,
    viewGetOverviewCoverage,
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
    FEATURE_FETCH_DEBOUNCE_MS,
    FETCH_DEBOUNCE_IDLE_MS,
    GUTTER_WIDTH,
    KEY_PAN_FRACTION,
    MAX_SEQUENCE_WINDOW_BP,
    MIN_VISIBLE_BP,
    NAV_IDLE_MS,
    PAD_L,
    PAD_R,
    OVERVIEW_BINS,
    READ_FETCH_MAX_BP,
    READ_LANE_H,
    READ_SEQUENCES_MAX_BP,
    READS_HEADER_H,
    SEQ_PREFETCH_CHUNK_BP,
    SEQ_PREFETCH_PAD_BP,
    VIEW_SIDE_PAD_FRACTION,
    ZOOM_IN_FACTOR,
    ZOOM_OUT_FACTOR,
  } from "$lib/view/constants";
  import {
    computeCoverageFetchWindow,
    computeReadFetchWindow,
    coverageCacheCovers,
    coverageCacheDenseEnough,
    desiredCoverageBinCount,
    maxDepthInView,
    readCacheCanDisplay,
    readCacheUsable,
    cacheNearEdge,
  } from "$lib/view/fetch";
  import {
    contigNameAliases,
    featureTypesForFilter,
    findContigLength,
    resolveContigName,
  } from "$lib/view/contig";
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
    type PackReadsResult,
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
    /** Whether this sample includes per-base sequence/qualities. */
    hasSequences: boolean;
  };

  type CoverageCache = {
    path: string;
    contig: string;
    /** Filters alter depth, so coverage must never be shared across them. */
    filterKey: string;
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

  type AlignTrackState = {
    path: string;
    label: string;
    doc: AlignmentDocument;
    visible: boolean;
    coverageBins: CoverageBin[];
    coverageMax: number;
    coverageCache: CoverageCache | null;
    overviewBins: CoverageBin[];
    overviewMax: number;
    /** Filters alter the whole-contig depth profile as well. */
    overviewFilterKey: string;
    alignmentReads: AlignmentRead[];
    readsTruncated: boolean;
    readsTotal: number;
    readCache: ReadCache | null;
    readPack: PackReadsResult;
  };

  let seqDoc = $state<SequenceDocument | null>(null);
  let annDocs = $state<AnnotationDocument[]>([]);
  let annTrackStates = $state<AnnTrackState[]>([]);
  let alignTracks = $state<AlignTrackState[]>([]);
  let featureTypeFilter = $state<"all" | "genes" | "exons" | "cds">("all");
  const alignDoc = $derived(alignTracks[0]?.doc ?? null);
  let selectedFeature = $state<AnnotationFeature | null>(null);
  const coverageBins = $derived(alignTracks[0]?.coverageBins ?? []);
  const coverageMax = $derived(alignTracks[0]?.coverageMax ?? 0);
  const alignmentReads = $derived(alignTracks[0]?.alignmentReads ?? []);
  const readsTruncated = $derived(alignTracks[0]?.readsTruncated ?? false);
  const readsTotal = $derived(alignTracks[0]?.readsTotal ?? 0);
  const readCache = $derived(alignTracks[0]?.readCache ?? null);
  const coverageCache = $derived(alignTracks[0]?.coverageCache ?? null);
  let selectedRead = $state<AlignmentRead | null>(null);
  let selectedCoverage = $state<CoverageBin | null>(null);

  let showCoverage = $state(true);
  let showSeqTrackVisible = $state(true);
  let showReadPileup = $state(true);
  let hideSecondary = $state(true);
  let hideSupplementary = $state(true);
  let hideDuplicates = $state(true);
  let minMapq = $state(0);
  /** Default: uniform grey (IGV-style); strand/MAPQ/pair available in Filters. */
  let colorReadsBy = $state<"strand" | "mapq" | "pair" | "none">("none");
  /** Mismatch highlights on reads when zoomed in (coverage stays simple grey depth). */
  let colorMismatches = $state(true);
  let colorBases = $state(true);
  let reverseComplement = $state(false);

  let contig = $state("");
  let viewStart = $state(0);
  let visibleBp = $state(100);
  let slice = $state<SequenceSlice | null>(null);

  let isLoading = $state(false);
  let isFetchingSlice = $state(false);
  let isFetchingFeatures = $state(false);
  let isFetchingAlign = $state(false);
  /** If a fetch was skipped because one was already in flight, re-run once it ends. */
  let alignFetchNeedsRerun = false;
  let errorMessage = $state("");
  let statusMessage = $state("Open FASTA, GFF/BED, and/or indexed BAM/CRAM.");
  let pendingCramPath = $state<string | null>(null);

  let canvasEl = $state<HTMLCanvasElement | null>(null);
  let readsCanvasEl = $state<HTMLCanvasElement | null>(null);
  let wrapEl = $state<HTMLDivElement | null>(null);
  let plotsEl = $state<HTMLDivElement | null>(null);
  /** Reactive plot width — updated via ResizeObserver so the canvas tracks window/pane size. */
  let layoutWidth = $state(640);
  let locusInput = $state("");
  let locusEditing = $state(false);
  let openMenu = $state<"display" | "filters" | null>(null);

  let selectionStart = $state<number | null>(null);
  let selectionEnd = $state<number | null>(null);
  let dragOrigin = $state<number | null>(null);
  let isPanning = $state(false);
  let isOverviewDragging = $state(false);
  /**
   * True during continuous navigation (wheel pan/zoom, arrow keys, zoom buttons).
   * Pointer drag uses isPanning / isOverviewDragging instead.
   * While true, never start uncancelable BAM/CRAM/FASTA IPC — paint from cache only.
   */
  let isNavigating = $state(false);
  let panOriginX = $state(0);
  let panOriginStart = $state(0);

  let showOverviewDepth = $state(false);
  let disposed = false;
  let fetchToken = 0;
  let featureFetchToken = 0;
  let alignFetchToken = 0;
  let localBuffer: LocalBuffer | null = null;
  let prefetchTimer: ReturnType<typeof setTimeout> | null = null;
  let alignDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let featureDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let navIdleTimer: ReturnType<typeof setTimeout> | null = null;
  /** Coalesce high-frequency pan/wheel viewStart updates to one per frame. */
  let panRaf = 0;
  let pendingViewStart: number | null = null;
  /**
   * Frozen pileup packing while the user navigates. Re-packing thousands of
   * reads on every pointermove monopolizes the webview event loop and makes
   * the file browser / menus feel dead.
   */
  const readPack = $derived(
    alignTracks[0]?.readPack ?? { packed: [], laneCount: 1, hiddenCount: 0 },
  );
  let paintRaf = 0;
  let paintReadsRaf = 0;

  /** Any active gesture that should block disk fetches. */
  function viewportBusy(): boolean {
    return isPanning || isOverviewDragging || isNavigating;
  }

  function emptyPack(): PackReadsResult {
    return { packed: [], laneCount: 1, hiddenCount: 0 };
  }

  function recomputeReadPack() {
    if (!showReadPileup || alignTracks.length === 0 || visibleBp > READ_FETCH_MAX_BP) {
      alignTracks = alignTracks.map((t) => ({ ...t, readPack: emptyPack() }));
      return;
    }
    alignTracks = alignTracks.map((t) => ({
      ...t,
      readPack: packReadsSquish(t.alignmentReads, viewStart, viewEnd),
    }));
  }

  /**
   * Apply a new viewStart, optionally coalesced to animation frames so
   * Svelte/$effect/paint run at most ~60×/s during continuous gestures.
   */
  function setViewStart(next: number, coalesce: boolean) {
    if (!coalesce) {
      if (panRaf) {
        cancelAnimationFrame(panRaf);
        panRaf = 0;
      }
      pendingViewStart = null;
      viewStart = next;
      clampWindow();
      return;
    }
    pendingViewStart = next;
    if (panRaf) return;
    panRaf = requestAnimationFrame(() => {
      panRaf = 0;
      if (pendingViewStart == null) return;
      viewStart = pendingViewStart;
      pendingViewStart = null;
      clampWindow();
    });
  }

  /**
   * Mark continuous nav (wheel/keys/zoom). Cancels pending fetch timers and
   * only schedules one network refresh after the user pauses.
   */
  function markNavigating() {
    isNavigating = true;
    if (navIdleTimer) clearTimeout(navIdleTimer);
    // Drop any pending network work — results of in-flight jobs are discarded
    // via tokens when they return; we just refuse to *start* more while busy.
    if (alignDebounceTimer) {
      clearTimeout(alignDebounceTimer);
      alignDebounceTimer = null;
    }
    if (prefetchTimer) {
      clearTimeout(prefetchTimer);
      prefetchTimer = null;
    }
    if (featureDebounceTimer) {
      clearTimeout(featureDebounceTimer);
      featureDebounceTimer = null;
    }
    // Paint only — do not re-filter caches every wheel tick (main-thread thrash).
    schedulePaint();
    navIdleTimer = setTimeout(() => {
      isNavigating = false;
      navIdleTimer = null;
      // Flush any coalesced pan first.
      if (pendingViewStart != null) {
        viewStart = pendingViewStart;
        pendingViewStart = null;
        clampWindow();
      }
      applyAlignmentFromCaches();
      recomputeReadPack();
      if (seqDoc) refreshVisibleSlice();
      if (featureDebounceTimer) clearTimeout(featureDebounceTimer);
      featureDebounceTimer = setTimeout(() => void refreshFeatures(), FEATURE_FETCH_DEBOUNCE_MS);
      refreshAlignmentTracksDebounced();
    }, NAV_IDLE_MS);
  }

  function contigsShareName(
    a: { name: string }[],
    b: { name: string }[],
  ): string | null {
    for (const ac of a) {
      for (const alias of contigNameAliases(ac.name)) {
        if (b.some((bc) => bc.name === alias || bc.name.toLowerCase() === alias.toLowerCase())) {
          return ac.name;
        }
      }
    }
    return null;
  }

  const contigLength = $derived.by(() => {
    const fromSeq = findContigLength(seqDoc?.contigs, contig);
    if (fromSeq && fromSeq > 0) return fromSeq;
    let fromAln: number | null = null;
    for (const t of alignTracks) {
      fromAln = findContigLength(t.doc.contigs, contig);
      if (fromAln) break;
    }
    if (fromAln && fromAln > 0) return fromAln;
    let max = 0;
    for (const doc of annDocs) {
      const spanLen = findContigLength(
        (doc.contigSpans ?? []).map((s) => ({ name: s.name, length: s.length })),
        contig,
      );
      if (spanLen) max = Math.max(max, spanLen);
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
  const selectedAlignmentPaths = $derived(
    selectedPaths.filter((path) => /\.(bam|cram)$/i.test(path)),
  );
  const selectedAlignmentPath = $derived(selectedAlignmentPaths[0] ?? null);
  const selectedIsCram = $derived(selectedAlignmentPaths.some((p) => /\.cram$/i.test(p)));
  const openIsCram = $derived(
    alignTracks.some((t) => t.doc.requiresReference || /\.cram$/i.test(t.doc.path)),
  );
  const showCramReferenceField = $derived(selectedIsCram || openIsCram || !!pendingCramPath);
  const cramReferenceReady = $derived(referencePath.trim().length > 0);

  const contigOptions = $derived.by(() => {
    if (seqDoc) return seqDoc.contigs.map((c) => ({ name: c.name, length: c.length }));
    if (alignTracks.length > 0) {
      const map = new Map<string, number>();
      for (const t of alignTracks) {
        for (const c of t.doc.contigs) map.set(c.name, Math.max(map.get(c.name) ?? 0, c.length));
      }
      return [...map.entries()].map(([name, length]) => ({ name, length }));
    }
    const map = new Map<string, number>();
    for (const doc of annDocs) {
      for (const span of doc.contigSpans ?? []) {
        map.set(span.name, Math.max(map.get(span.name) ?? 0, span.length));
      }
    }
    return [...map.entries()].map(([name, length]) => ({ name, length }));
  });

  const hasDocument = $derived(!!seqDoc || annDocs.length > 0 || alignTracks.length > 0);
  const seqAvailable = $derived(hasDocument);
  const covAvailable = $derived(alignTracks.length > 0);
  const readsAvailable = $derived(alignTracks.length > 0);
  const showSeqTrack = $derived(seqAvailable && showSeqTrackVisible);
  const showCovTrack = $derived(covAvailable && showCoverage);
  const showReadsTrack = $derived(readsAvailable && showReadPileup);

  const plotWidth = $derived(Math.max(200, layoutWidth));

  function coveragePlanForView(): { start: number; end: number; binCount: number } {
    const { start, end } = computeCoverageFetchWindow(
      viewStart,
      viewEnd,
      contigLength,
      visibleBp,
    );
    const binCount = desiredCoverageBinCount(end - start, plotWidth, visibleBp);
    return { start, end, binCount };
  }

  function layoutTracksInput() {
    const densityOnly = visibleBp > READ_FETCH_MAX_BP;
    return {
      seqAvailable,
      seqExpanded: showSeqTrackVisible,
      covTracks: alignTracks.map((t) => ({
        path: t.path,
        label: t.label,
        expanded: showCoverage && t.visible,
      })),
      annTracks: annTrackStates.map((t) => ({
        path: t.path,
        label: t.label,
        expanded: t.visible,
        laneCount: estimateFeatureLanes(t.features, viewStart, viewEnd),
      })),
      readsTracks: alignTracks.map((t) => ({
        path: t.path,
        label: t.label,
        expanded: showReadPileup && t.visible,
        laneCount: t.readPack.laneCount,
        densityOnly,
      })),
    };
  }

  // Recompute pileup packing only when idle — freezes lane assignment during pan/zoom.


  function measurePlotWidth() {
    const el = plotsEl ?? wrapEl;
    if (!el) return;
    const w = el.clientWidth;
    // When measuring the full viewer, subtract the gutter.
    const next = Math.max(
      200,
      plotsEl ? w : Math.max(0, w - GUTTER_WIDTH),
    );
    if (Math.abs(next - layoutWidth) >= 1) {
      layoutWidth = next;
    }
  }

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
  // The initial orientation is always forward; keep this plain value rather
  // than capturing a reactive state value during component construction.
  let lastRc = false;
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
    featureTypeFilter;
    // Track annotation *document* list (open/close), not per-track feature arrays.
    annDocs;
    referencePath;
    showOverviewDepth;
    if (reverseComplement !== lastRc) {
      lastRc = reverseComplement;
      localBuffer = null;
    }
    // Skip all disk IPC while the user is still navigating (pan/zoom/wheel/keys).
    // Continuous gestures only paint from cache; markNavigating() re-fetches on idle.
    if (viewportBusy()) {
      schedulePaint();
      return;
    }
    // Cache/result reads are implementation details, not load-effect dependencies.
    untrack(() => {
      if (seqDoc) refreshVisibleSlice();
      if (featureDebounceTimer) clearTimeout(featureDebounceTimer);
      featureDebounceTimer = setTimeout(() => void refreshFeatures(), FEATURE_FETCH_DEBOUNCE_MS);
      applyAlignmentFromCaches();
      refreshAlignmentTracksDebounced();
    });
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
    showOverviewDepth;
    plotWidth;
    // While navigating, paint is already scheduled from setViewStart/markNavigating —
    // avoid a second full reactive storm from every dependency listed above.
    if (viewportBusy()) {
      schedulePaint();
      return;
    }
    schedulePaint();
  });

  // When referencePath is set externally (e.g. right-click → Set as reference),
  // put that FASTA on the Reference track. Also open pending CRAM if waiting.
  // Guard with lastRefTrackPath so we do not re-open a multi-GB FASTA on every effect tick.
  let lastRefTrackPath = "";
  $effect(() => {
    const ref = referencePath.trim();
    if (!ref || isLoading) return;
    const want = ref.replace(/\\/g, "/");
    if (want === lastRefTrackPath) {
      if (pendingCramPath) void openAlignmentPath(pendingCramPath);
      return;
    }
    lastRefTrackPath = want;
    if (pendingCramPath) {
      void openAlignmentPath(pendingCramPath);
    }
    void loadReferenceOntoTrack(ref);
  });

  /** Active pointer id we captured on a canvas — must always be released or the
   * rest of the app (file browser, mode tabs) stops receiving clicks. */
  let capturedPointerId: number | null = null;
  let captureTarget: HTMLCanvasElement | null = null;
  /** True while we intentionally release capture (suppress lostpointercapture re-entry). */
  let releasingCapture = false;
  /** Ignore RO width jitter smaller than this (scrollbar appear/disappear thrash). */
  let lastObservedPlotWidth = 0;

  function releaseCanvasPointerCapture() {
    if (captureTarget && capturedPointerId != null) {
      releasingCapture = true;
      try {
        if (captureTarget.hasPointerCapture(capturedPointerId)) {
          captureTarget.releasePointerCapture(capturedPointerId);
        }
      } catch {
        /* already released */
      } finally {
        releasingCapture = false;
      }
    }
    capturedPointerId = null;
    captureTarget = null;
  }

  /** End pan/select gesture and free the rest of the UI to receive events. */
  function endCanvasGesture(opts?: { fromWindow?: boolean }) {
    const wasPanning = isPanning || isOverviewDragging;
    const wasSelecting = dragOrigin != null;
    if (!wasPanning && !wasSelecting && capturedPointerId == null) return;
    releaseCanvasPointerCapture();
    isPanning = false;
    isOverviewDragging = false;
    dragOrigin = null;
    if (pendingViewStart != null) {
      viewStart = pendingViewStart;
      pendingViewStart = null;
      clampWindow();
    }
    if (panRaf) {
      cancelAnimationFrame(panRaf);
      panRaf = 0;
    }
    if (wasPanning) {
      applyAlignmentFromCaches();
      recomputeReadPack();
      if (seqDoc) refreshVisibleSlice();
      refreshAlignmentTracksDebounced();
    } else if (wasSelecting || opts?.fromWindow) {
      schedulePaint();
    }
  }

  onMount(() => {
    const onResize = () => {
      measurePlotWidth();
      schedulePaint();
    };
    const onTheme = () => schedulePaint();
    // When the window is backgrounded, stop thrashing the event loop.
    const onVisibility = () => {
      if (document.hidden) {
        endCanvasGesture({ fromWindow: true });
        if (paintRaf) cancelAnimationFrame(paintRaf);
        paintRaf = 0;
        if (paintReadsRaf) cancelAnimationFrame(paintReadsRaf);
        paintReadsRaf = 0;
      } else {
        schedulePaint();
      }
    };
    // Safety net: canvas pointerup can be lost (alt-tab, OS grab). Without this,
    // setPointerCapture steals every click from the file browser and menus.
    const onWindowPointerEnd = (event: PointerEvent) => {
      if (capturedPointerId == null) return;
      if (event.pointerId !== capturedPointerId) return;
      endCanvasGesture({ fromWindow: true });
    };
    const onWindowBlur = () => endCanvasGesture({ fromWindow: true });
    window.addEventListener("resize", onResize);
    window.addEventListener("helixgt-theme", onTheme);
    window.addEventListener("pointerup", onWindowPointerEnd, true);
    window.addEventListener("pointercancel", onWindowPointerEnd, true);
    window.addEventListener("blur", onWindowBlur);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      disposed = true;
      fetchToken++;
      featureFetchToken++;
      alignFetchToken++;
      window.removeEventListener("resize", onResize);
      window.removeEventListener("helixgt-theme", onTheme);
      window.removeEventListener("pointerup", onWindowPointerEnd, true);
      window.removeEventListener("pointercancel", onWindowPointerEnd, true);
      window.removeEventListener("blur", onWindowBlur);
      document.removeEventListener("visibilitychange", onVisibility);
      endCanvasGesture({ fromWindow: true });
      if (prefetchTimer) clearTimeout(prefetchTimer);
      if (alignDebounceTimer) clearTimeout(alignDebounceTimer);
      if (featureDebounceTimer) clearTimeout(featureDebounceTimer);
      if (navIdleTimer) clearTimeout(navIdleTimer);
      if (panRaf) cancelAnimationFrame(panRaf);
      if (paintRaf) cancelAnimationFrame(paintRaf);
      if (paintReadsRaf) cancelAnimationFrame(paintReadsRaf);
    };
  });

  // Keep layoutWidth in sync with the actual plot area (window + files pane + mode rail).
  $effect(() => {
    const el = plotsEl ?? wrapEl;
    if (!el) return;
    measurePlotWidth();
    lastObservedPlotWidth = el.clientWidth;
    let roRaf = 0;
    const ro = new ResizeObserver((entries) => {
      // Width-only: canvas height changes must NOT re-trigger layout (scrollbar loops).
      const entry = entries[0];
      const w = entry?.contentRect?.width ?? el.clientWidth;
      if (Math.abs(w - lastObservedPlotWidth) < 2) return;
      lastObservedPlotWidth = w;
      if (roRaf) return;
      roRaf = requestAnimationFrame(() => {
        roRaf = 0;
        measurePlotWidth();
        schedulePaint();
      });
    });
    ro.observe(el);
    return () => {
      ro.disconnect();
      if (roRaf) cancelAnimationFrame(roRaf);
    };
  });

  function schedulePaint() {
    if (document.hidden) return;
    if (paintRaf) return;
    // During continuous nav: paint next frame (snappy). When idle: one extra
    // rAF so shell click handlers aren't starved behind canvas work.
    paintRaf = requestAnimationFrame(() => {
      paintRaf = 0;
      if (viewportBusy()) {
        paint();
        return;
      }
      paintRaf = requestAnimationFrame(() => {
        paintRaf = 0;
        paint();
      });
    });
  }

  /** Let the browser process clicks / paints between heavy IPC apply steps. */
  function yieldToBrowser(): Promise<void> {
    return new Promise((resolve) => {
      setTimeout(resolve, 0);
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
    computeUnifiedLayout(plotWidth, layoutTracksInput()),
  );

  function paint() {
    if (!canvasEl) return;
    if (document.hidden) return;
    const layout = computeUnifiedLayout(plotWidth, layoutTracksInput());
    const cssW = plotWidth;
    const busy = viewportBusy();
    const overviewSrc = alignTracks.find((t) => t.overviewBins.length > 0) ?? alignTracks[0];
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
      coverageTracks: alignTracks.map((t) => ({
        path: t.path,
        bins: t.coverageBins,
        maxDepth: t.coverageMax,
      })),
      overviewBins: showOverviewDepth ? (overviewSrc?.overviewBins ?? []) : [],
      overviewMax: overviewSrc?.overviewMax ?? 0,
      isFetchingAlign,
      annTracks: annTrackStates
        .filter((t) => t.visible)
        .map((t) => ({
          path: t.path,
          features: t.features,
          truncated: t.truncated,
        })),
      selectedFeature,
      selectedCoverage,
      selectionStart,
      selectionEnd,
      colorBases,
    });

    if (!readsAvailable || !readsCanvasEl) return;

    const packedCount = alignTracks.reduce((n, t) => n + t.readPack.packed.length, 0);
    const paintReads = () => {
      if (!readsCanvasEl) return;
      paintReadsTrack({
        canvas: readsCanvasEl,
        layout,
        cssW,
        viewStart,
        viewEnd,
        visibleBp,
        tracks: alignTracks.map((t) => ({
          path: t.path,
          packed: showReadPileup ? t.readPack.packed : [],
          hiddenCount: showReadPileup ? t.readPack.hiddenCount : 0,
          truncated: t.readsTruncated,
          total: t.readsTotal,
          coverageBins: t.coverageBins,
          coverageMax: t.coverageMax,
        })),
        localBuffer,
        contig,
        isFetchingAlign,
        selectedRead,
        colorReadsBy,
        colorBases,
        colorMismatches,
        selectionStart,
        selectionEnd,
        isPanning: busy,
      });
    };

    if (busy || packedCount < 400) {
      if (paintReadsRaf) {
        cancelAnimationFrame(paintReadsRaf);
        paintReadsRaf = 0;
      }
      paintReads();
    } else {
      if (paintReadsRaf) cancelAnimationFrame(paintReadsRaf);
      paintReadsRaf = requestAnimationFrame(() => {
        paintReadsRaf = 0;
        paintReads();
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
      const label = doc.path.split(/[\\/]/).pop() ?? doc.path;
      const next: AlignTrackState = {
        path: doc.path,
        label,
        doc,
        visible: true,
        coverageBins: [],
        coverageMax: 0,
        coverageCache: null,
        overviewBins: [],
        overviewMax: 0,
        overviewFilterKey: "",
        alignmentReads: [],
        readsTruncated: false,
        readsTotal: 0,
        readCache: null,
        readPack: emptyPack(),
      };
      alignTracks = [...alignTracks.filter((t) => t.path !== doc.path), next];
      pendingCramPath = null;
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
      // Load a FASTA into the Reference track if the user has not opened one yet.
      await ensureReferenceSequenceLoaded();
      void refreshAlignmentTracks();
    } catch (error) {
      if (isCram) pendingCramPath = path;
      errorMessage = String(error);
      onLog(`Alignment open failed: ${String(error)}`, "error");
    } finally {
      isLoading = false;
    }
  }

  function pathsEqualLoose(a: string, b: string): boolean {
    const norm = (p: string) => p.replace(/\\/g, "/").toLowerCase();
    return norm(a) === norm(b);
  }

  /**
   * Load a FASTA onto the dedicated Reference track without resetting the locus
   * when contig names still match (e.g. swapping reference while viewing BAM).
   */
  async function loadReferenceOntoTrack(path: string) {
    const candidate = path.trim();
    if (!candidate) return;
    // Already on this reference (including fai-backed opens with no full in-memory cache).
    if (seqDoc && pathsEqualLoose(seqDoc.path, candidate)) {
      showSeqTrackVisible = true;
      void prefetchAroundViewport(true);
      return;
    }
    try {
      statusMessage = `Opening reference ${candidate}…`;
      const doc = await viewOpenDocument(candidate);
      seqDoc = doc;
      referencePath = candidate;
      lastRefTrackPath = candidate.replace(/\\/g, "/");
      localBuffer = null;
      slice = null;
      showSeqTrackVisible = true;
      errorMessage = "";

      // Prefer keeping the current contig when names match (incl. chr aliases).
      const currentOk =
        !!contig &&
        (doc.contigs.some((c) => c.name === contig) ||
          findContigLength(doc.contigs, contig) != null);
      if (currentOk) {
        // Keep current contig / window.
      } else if (alignDoc) {
        // Prefer alignment contig name when a FASTA contig aliases to it.
        const alnMatch = alignDoc.contigs.find(
          (a) => findContigLength(doc.contigs, a.name) != null,
        );
        if (alnMatch) contig = alnMatch.name;
        else {
          const shared = contigsShareName(doc.contigs, alignDoc.contigs);
          if (shared) contig = shared;
          else if (doc.contigs[0]) contig = doc.contigs[0].name;
        }
      } else if (doc.contigs[0]) {
        contig = doc.contigs[0].name;
        viewStart = 0;
        visibleBp = defaultVisibleBp(doc.contigs[0].length);
      }

      clampWindow();
      void prefetchAroundViewport(true);
      const idxNote = doc.fullyCached ? "" : " (indexed)";
      onLog(
        `Reference track: ${candidate} — ${doc.contigs.length} contig(s), ${doc.totalBases.toLocaleString()} bases${idxNote}`,
      );
      statusMessage = `Reference · ${doc.contigs.length} contig(s) · ${doc.totalBases.toLocaleString()} bases${idxNote}`;
    } catch (error) {
      const msg = String(error);
      errorMessage = msg;
      statusMessage = "Failed to load reference FASTA.";
      onLog(`Could not load reference FASTA for Reference track: ${msg}`, "error");
    }
  }

  /**
   * Ensure the dedicated Reference track has sequence data. Prefer an already-open
   * FASTA; otherwise use the CRAM/reference path or a selected FASTA from the tree.
   */
  async function ensureReferenceSequenceLoaded() {
    if (seqDoc) {
      void prefetchAroundViewport(true);
      return;
    }
    const candidate = referencePath.trim() || selectedSequencePath || null;
    if (!candidate) return;
    await loadReferenceOntoTrack(candidate);
  }

  async function browseReferenceFasta() {
    const picked = await open({
      directory: false,
      multiple: false,
      title: "Choose reference FASTA",
      filters: [{ name: "FASTA", extensions: ["fasta", "fa", "fna", "fa.gz", "fasta.gz", "fna.gz"] }],
    });
    if (picked) {
      // Setting referencePath triggers the effect → loadReferenceOntoTrack.
      referencePath = String(picked);
      onLog(`Reference set to ${referencePath}`);
      if (pendingCramPath) void openAlignmentPath(pendingCramPath);
    }
  }

  async function openSelected() {
    const seq = selectedSequencePath;
    const anns = selectedAnnotationPaths;
    if (!seq && anns.length === 0 && selectedAlignmentPaths.length === 0) {
      onLog("Select FASTA/FASTQ, GFF/BED, and/or BAM/CRAM first.", "error");
      return;
    }
    // Open FASTA first so the Reference track is ready before alignments paint.
    if (seq) await openSequencePath(seq);
    for (const path of anns) await openAnnotationPath(path);
    for (const path of selectedAlignmentPaths) await openAlignmentPath(path);
    if (!seqDoc) await ensureReferenceSequenceLoaded();
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
    alignTracks = [];
    pendingCramPath = null;
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

  /** Desired sequence window: modest pad for snappy pan (not multi-Mb chunks). */
  function desiredSequenceWindow(): { start: number; end: number } {
    if (contigLength > 0 && contigLength <= SEQ_PREFETCH_CHUNK_BP) {
      return { start: 0, end: contigLength };
    }
    // Prefer ~viewport + pad, capped — huge ref slices freeze the UI on IPC.
    const pad = Math.max(
      Math.min(SEQ_PREFETCH_PAD_BP, 60_000),
      Math.floor(visibleBp * VIEW_SIDE_PAD_FRACTION),
    );
    let start = Math.max(0, viewStart - pad);
    let end = Math.min(contigLength || viewEnd + pad, viewEnd + pad);
    const maxSpan = Math.min(
      MAX_SEQUENCE_WINDOW_BP,
      Math.max(visibleBp + pad * 2, Math.min(SEQ_PREFETCH_CHUNK_BP, 150_000)),
    );
    if (end - start > maxSpan) {
      const center = viewStart + visibleBp / 2;
      start = Math.max(0, Math.floor(center - maxSpan / 2));
      end = Math.min(contigLength || start + maxSpan, start + maxSpan);
      start = Math.max(0, end - maxSpan);
    }
    return { start, end };
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
      // Keep painting from cache — never blank the track while prefetch runs.
      slice = local;
      maybeSchedulePrefetch();
      return;
    }
    // Do not start FASTA IPC mid-gesture (uncancelable spawn_blocking).
    if (viewportBusy()) return;
    // Viewport not fully covered: fetch expanded window without clearing old paint
    // unless we have nothing usable at all.
    void fetchExpandedWindow();
  }

  function maybeSchedulePrefetch() {
    if (!localBuffer) return;
    if (viewportBusy()) return;
    const margin = Math.max(
      Math.floor(visibleBp * VIEW_SIDE_PAD_FRACTION * 0.4),
      Math.min(SEQ_PREFETCH_PAD_BP, Math.floor((localBuffer.end - localBuffer.start) * 0.2)),
    );
    const nearLeft = viewStart - localBuffer.start < margin;
    const nearRight = localBuffer.end - viewEnd < margin;
    if (nearLeft || nearRight) {
      if (prefetchTimer) clearTimeout(prefetchTimer);
      prefetchTimer = setTimeout(() => void prefetchAroundViewport(false), 50);
    }
  }

  async function prefetchAroundViewport(force: boolean) {
    if (!seqDoc || !contig || contigLength <= 0) return;
    if (visibleBp > MAX_SEQUENCE_WINDOW_BP) return;
    // Never fight the UI with IPC while navigating.
    if (!force && viewportBusy()) return;
    const { start, end } = desiredSequenceWindow();
    if (
      !force &&
      localBuffer &&
      localBuffer.contig === contig &&
      localBuffer.reverseComplement === reverseComplement &&
      localBuffer.start <= start &&
      localBuffer.end >= end
    ) {
      return;
    }
    await loadBuffer(start, end, force);
  }

  async function fetchExpandedWindow() {
    const { start, end } = desiredSequenceWindow();
    await loadBuffer(start, end, true);
  }

  function mergeSequenceBuffers(
    prev: LocalBuffer | null,
    next: LocalBuffer,
  ): LocalBuffer {
    if (
      !prev ||
      prev.contig !== next.contig ||
      prev.reverseComplement !== next.reverseComplement
    ) {
      return next;
    }
    // No overlap / adjacency — prefer the new window (centered on viewport).
    if (next.end < prev.start || next.start > prev.end) {
      return next;
    }
    const start = Math.min(prev.start, next.start);
    const end = Math.max(prev.end, next.end);
    if (end - start > MAX_SEQUENCE_WINDOW_BP) {
      return next;
    }
    // Stitch without gaps (buffers overlap).
    let sequence = "";
    for (let p = start; p < end; ) {
      const inNext = p >= next.start && p < next.end;
      const inPrev = p >= prev.start && p < prev.end;
      if (inNext) {
        const from = p - next.start;
        const to = Math.min(next.end, end) - next.start;
        sequence += next.sequence.slice(from, to);
        p = Math.min(next.end, end);
      } else if (inPrev) {
        const from = p - prev.start;
        const to = Math.min(prev.end, end) - prev.start;
        sequence += prev.sequence.slice(from, to);
        p = Math.min(prev.end, end);
      } else {
        // Should not happen when intervals merge continuously.
        break;
      }
    }
    if (sequence.length !== end - start) {
      return next;
    }
    return {
      contig: next.contig,
      start,
      end,
      sequence,
      reverseComplement: next.reverseComplement,
    };
  }

  async function loadBuffer(start: number, end: number, updateVisible: boolean) {
    if (!seqDoc || !contig || end <= start) return;
    const token = ++fetchToken;
    const requestedPath = seqDoc.path;
    const requestedContig = contig;
    const requestedRc = reverseComplement;
    const current = () => token === fetchToken && seqDoc?.path === requestedPath
      && contig === requestedContig && reverseComplement === requestedRc;
    // Only surface “fetching…” for visible loads — background prefetch must not
    // thrash Svelte re-renders of the whole pane (and the rest of the app).
    if (updateVisible) isFetchingSlice = true;
    try {
      const next = await viewGetSequenceWindow({
        path: seqDoc.path,
        contig,
        start,
        end,
        reverseComplement,
      });
      if (!current()) return;
      await yieldToBrowser();
      if (!current()) return;
      const incoming: LocalBuffer = {
        contig,
        start: next.start,
        end: next.end,
        sequence: next.sequence,
        reverseComplement,
      };
      localBuffer = mergeSequenceBuffers(localBuffer, incoming);
      if (updateVisible) {
        slice =
          sliceFromLocalBuffer() ??
          ({
            contig: next.contig,
            start: Math.max(viewStart, next.start),
            end: Math.min(viewEnd, next.end),
            sequence: next.sequence.slice(
              Math.max(0, viewStart - next.start),
              Math.max(0, viewEnd - next.start),
            ),
            contigLength: next.contigLength,
            reverseComplemented: next.reverseComplemented,
          } satisfies SequenceSlice);
      }
    } catch (error) {
      if (!current()) return;
      // Keep any existing buffer/slice so the track does not blink empty.
      if (updateVisible && !sliceFromLocalBuffer()) {
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
    if (viewportBusy()) return;
    clampWindow();
    const pad = Math.max(visibleBp, 1000);
    const start = Math.max(0, viewStart - pad);
    const end = Math.min(contigLength, viewEnd + pad);
    const token = ++featureFetchToken;
    const docs = [...annDocs];
    const requestedContig = contig;
    const requestedFilter = featureTypeFilter;
    const current = () => token === featureFetchToken && contig === requestedContig
      && featureTypeFilter === requestedFilter && docs.length === annDocs.length
      && docs.every((doc, i) => doc === annDocs[i]);
    isFetchingFeatures = true;
    try {
      const windows = await Promise.all(
        docs.map((doc) =>
          viewGetFeaturesInRange({
            path: doc.path,
            contig,
            start,
            end,
            featureTypes: featureTypesForFilter(featureTypeFilter),
          }),
        ),
      );
      if (!current()) return;
      const nextTracks: AnnTrackState[] = docs.map((doc, i) => {
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
      if (!current()) return;
      onLog(`Feature fetch failed: ${String(error)}`, "warn");
    } finally {
      if (token === featureFetchToken) isFetchingFeatures = false;
    }
  }

  function alignmentFilterKey(includeSequences: boolean): string {
    return (
      referencePath.trim() + "|" + readFilterKey({
        hideSecondary,
        hideSupplementary,
        hideDuplicates,
        minMapq,
      }) + (includeSequences ? "|seq" : "|noseq")
    );
  }

  function filterOpts() {
    return {
      includeSecondary: !hideSecondary,
      includeSupplementary: !hideSupplementary,
      includeDuplicates: !hideDuplicates,
      minMapq,
    };
  }

  function trackNeedsNetwork(
    t: AlignTrackState,
    needSeq: boolean,
    readFilters: string,
    coverageFilters: string,
  ): boolean {
    const wantReads = showReadPileup && visibleBp <= READ_FETCH_MAX_BP;
    const readsHit =
      !wantReads ||
      readCacheUsable(t.readCache, t.path, contig, readFilters, viewStart, viewEnd, needSeq);
    const covHit =
      !showCoverage ||
      (t.coverageCache?.filterKey === coverageFilters &&
        coverageCacheCovers(t.coverageCache, t.path, contig, viewStart, viewEnd) &&
        coverageCacheDenseEnough(t.coverageCache, viewStart, viewEnd, plotWidth));
    return (showCoverage && !covHit) || (wantReads && !readsHit);
  }

  function refreshAlignmentTracksDebounced() {
    if (alignDebounceTimer) clearTimeout(alignDebounceTimer);
    applyAlignmentFromCaches();
    schedulePaint();
    if (viewportBusy()) return;

    const needSeq = visibleBp <= READ_SEQUENCES_MAX_BP;
    const readFilters = alignmentFilterKey(needSeq);
    // Sequence payload is irrelevant to depth, so crossing the read-detail zoom
    // threshold must not trigger a new coverage or overview scan.
    const coverageFilters = alignmentFilterKey(false);
    const needsWork = alignTracks.some((t) =>
      trackNeedsNetwork(t, needSeq, readFilters, coverageFilters),
    );
    if (
      !needsWork &&
      (!showOverviewDepth || alignTracks.every(
        (t) => t.overviewFilterKey === coverageFilters,
      ))
    ) return;
    alignDebounceTimer = setTimeout(() => void refreshAlignmentTracks(), FETCH_DEBOUNCE_IDLE_MS);
  }

  function applyTrackFromCaches(
    t: AlignTrackState,
    needSeq: boolean,
    readFilters: string,
    coverageFilters: string,
  ): AlignTrackState {
    const wantReads = showReadPileup && visibleBp <= READ_FETCH_MAX_BP;
    let next = t;
    // A stale depth graph is worse than a short loading gap: its values no
    // longer match the visible MAPQ/flag controls. Keep the cache object for
    // comparison, but do not paint it until a matching result arrives.
    if (t.overviewFilterKey !== coverageFilters && t.overviewBins.length > 0) {
      next = { ...next, overviewBins: [], overviewMax: 0 };
    }
    if (
      showCoverage &&
      t.coverageCache?.filterKey === coverageFilters &&
      coverageCacheCovers(t.coverageCache, t.path, contig, viewStart, viewEnd)
    ) {
      const bins = t.coverageCache.bins.filter((b) => b.end > viewStart && b.start < viewEnd);
      next = {
        ...next,
        coverageBins: bins,
        coverageMax: maxDepthInView(bins, viewStart, viewEnd),
      };
    } else if (showCoverage && (t.coverageBins.length > 0 || t.coverageMax > 0)) {
      next = { ...next, coverageBins: [], coverageMax: 0 };
    }
    if (!wantReads) {
      return {
        ...next,
        alignmentReads: [],
        readsTruncated: false,
        readsTotal: 0,
      };
    }
    if (t.readCache && readCacheCanDisplay(t.readCache, t.path, contig, readFilters, viewStart, viewEnd)) {
      const overlap = readsOverlapWindow(t.readCache.reads, viewStart, viewEnd);
      next = {
        ...next,
        alignmentReads: overlap,
        readsTruncated: t.readCache.truncated,
        readsTotal: Math.max(t.readCache.totalInRange, overlap.length),
      };
    } else if (t.alignmentReads.length > 0) {
      next = { ...next, alignmentReads: [], readsTruncated: false, readsTotal: 0 };
    }
    return next;
  }

  function applyAlignmentFromCaches() {
    if (alignTracks.length === 0 || !contig) return;
    const needSeq = visibleBp <= READ_SEQUENCES_MAX_BP;
    const readFilters = alignmentFilterKey(needSeq);
    const coverageFilters = alignmentFilterKey(false);
    alignTracks = alignTracks.map((t) =>
      applyTrackFromCaches(t, needSeq, readFilters, coverageFilters),
    );
    if (selectedRead && !alignTracks.some((t) => t.alignmentReads.some((r) =>
      r.name === selectedRead!.name && r.start === selectedRead!.start && r.flags === selectedRead!.flags))) {
      selectedRead = null;
    }
    if (!viewportBusy()) recomputeReadPack();
  }

  const overviewRequests = new Map<string, string>();

  // Overview scans must never delay local coverage/pileup or the next pan.
  async function refreshOverview(t: AlignTrackState, requestedContig: string,
    length: number, filters: string, ref: string | null, opts: ReturnType<typeof filterOpts>) {
    const key = `${requestedContig}|${filters}`;
    if (!showOverviewDepth || t.overviewFilterKey === filters || overviewRequests.has(t.path)) return;
    overviewRequests.set(t.path, key);
    const current = () => !disposed && contig === requestedContig
      && showOverviewDepth && alignmentFilterKey(false) === filters && alignTracks.some((track) => track.doc === t.doc);
    try {
      const ov = await viewGetOverviewCoverage({ path: t.path, contig: requestedContig,
        contigLength: length, binCount: OVERVIEW_BINS, referencePath: ref, ...opts });
      if (!current()) return;
      alignTracks = alignTracks.map((track) => track === t || track.doc === t.doc
        ? { ...track, overviewBins: ov.bins, overviewMax: ov.maxDepth, overviewFilterKey: filters }
        : track);
      schedulePaint();
    } catch (error) {
      if (current()) {
        // Mark this attempt so a failed overview cannot create an automatic retry loop.
        alignTracks = alignTracks.map((track) => track.doc === t.doc
          ? { ...track, overviewFilterKey: filters } : track);
        onLog(`Overview unavailable (${t.label}): ${String(error)}`, "warn");
      }
    } finally {
      overviewRequests.delete(t.path);
      if (!disposed && !current()) refreshAlignmentTracksDebounced();
    }
  }

  async function refreshAlignmentTracks() {
    if (disposed) return;
    if (alignTracks.length === 0 || !contig || contigLength <= 0) return;
    if (viewportBusy()) { applyAlignmentFromCaches(); return; }
    if (isFetchingAlign) { alignFetchNeedsRerun = true; return; }
    clampWindow();
    const token = ++alignFetchToken;
    const requestedContig = contig;
    const requestedLength = contigLength;
    const includeSequences = visibleBp <= READ_SEQUENCES_MAX_BP;
    const readFilters = alignmentFilterKey(includeSequences);
    const coverageFilters = alignmentFilterKey(false);
    const covPlan = coveragePlanForView();
    const readWin = computeReadFetchWindow(viewStart, viewEnd, contigLength, visibleBp);
    const wantReads = showReadPileup && visibleBp <= READ_FETCH_MAX_BP;
    const opts = filterOpts();
    const reference = referencePath.trim() || null;
    const current = () => token === alignFetchToken && contig === requestedContig
      && alignmentFilterKey(false) === coverageFilters;
    applyAlignmentFromCaches();
    const tracks = [...alignTracks];
    isFetchingAlign = true;
    try {
      await Promise.all(tracks.map(async (t) => {
        const ref = t.doc.requiresReference ? reference : null;
        const readsHit = !wantReads || readCacheUsable(t.readCache, t.path, requestedContig,
          readFilters, viewStart, viewEnd, includeSequences);
        const covHit = !showCoverage || (t.coverageCache?.filterKey === coverageFilters
          && coverageCacheCovers(t.coverageCache, t.path, requestedContig, viewStart, viewEnd)
          && coverageCacheDenseEnough(t.coverageCache, viewStart, viewEnd, plotWidth));
        try {
          // Keep read queries tight: sampling the padded coverage window can
          // discard most reads at the actual locus and inflate sequence payloads.
          const [covResult, readsResult] = await Promise.allSettled([
            covHit ? null : viewGetCoverageBins({ path: t.path, contig: requestedContig,
              ...covPlan, referencePath: ref, ...opts }),
            readsHit ? null : viewGetReadsInRange({ path: t.path, contig: requestedContig,
              ...readWin, referencePath: ref, ...opts, includeSequences }),
          ]);
          if (!current()) return;
          for (const result of [covResult, readsResult]) {
            if (result.status === "rejected") onLog(`Alignment track failed (${t.label}): ${String(result.reason)}`, "warn");
          }
          const cov = covResult.status === "fulfilled" ? covResult.value : null;
          const reads = readsResult.status === "fulfilled" ? readsResult.value : null;
          // Merge into current tracks, preserving additions, removals and visibility.
          alignTracks = alignTracks.map((track) => track.doc !== t.doc ? track : {
            ...track,
            coverageCache: cov ? { path: t.path, contig: requestedContig,
              filterKey: coverageFilters, start: cov.start, end: cov.end,
              binCount: cov.bins.length, bins: cov.bins, maxDepth: cov.maxDepth } : track.coverageCache,
            readCache: reads ? { path: t.path, contig: requestedContig,
              start: reads.start, end: reads.end, filterKey: readFilters,
              reads: reads.reads, totalInRange: reads.totalInRange,
              truncated: reads.truncated, hasSequences: includeSequences } : track.readCache,
          });
          applyAlignmentFromCaches();
          schedulePaint();
          const track = alignTracks.find((track) => track.doc === t.doc);
          if (track) void refreshOverview(track, requestedContig, requestedLength, coverageFilters, ref, opts);
        } catch (error) {
          if (current()) onLog(`Alignment track failed (${t.label}): ${String(error)}`, "warn");
        }
      }));
    } finally {
      if (token === alignFetchToken) {
        isFetchingAlign = false;
        if (!current()) alignFetchNeedsRerun = true;
        applyAlignmentFromCaches();
        schedulePaint();
        if (alignFetchNeedsRerun) {
          alignFetchNeedsRerun = false;
          refreshAlignmentTracksDebounced();
        }
      }
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
    // Wheel/button zoom is not isPanning — still must suppress IPC until idle.
    markNavigating();
  }

  function applyLocusFromInput() {
    const colon = locusInput.indexOf(":");
    const requested = colon >= 0 ? locusInput.slice(0, colon).trim() || contig : contig;
    const target = resolveContigName(contigOptions.map((c) => c.name), requested);
    if (!target) { onLog(`Unknown contig: ${requested}`, "warn"); return; }
    const targetLength = contigOptions.find((c) => c.name === target)?.length ?? 0;
    const parsed = parseLocus(locusInput, target, targetLength);
    if (!parsed.ok) {
      onLog(`Locus: ${parsed.message}`, "warn");
      if (contig) locusInput = formatLocus(contig, viewStart, viewEnd);
      return;
    }
    if (parsed.contig !== contig) {
      const resolved = resolveContigName(
        contigOptions.map((c) => c.name),
        parsed.contig,
      );
      if (resolved) onContigChange(resolved);
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

  function overviewCenterAt(clientX: number, el: HTMLElement, coalesce = false) {
    if (contigLength <= 0) return;
    const layout = viewerLayout;
    const rect = el.getBoundingClientRect();
    const x = clientX - rect.left;
    const usable = Math.max(1, layout.usable);
    const t = Math.max(0, Math.min(1, (x - layout.padL) / usable));
    const center = Math.floor(t * contigLength);
    setViewStart(
      Math.max(0, Math.min(contigLength - visibleBp, center - Math.floor(visibleBp / 2))),
      coalesce,
    );
  }

  function onViewerKeydown(event: KeyboardEvent) {
    if (!hasDocument || disabled) return;
    const target = event.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA") {
      return;
    }
    if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      zoomInCenter(); // markNavigating via zoomAtClientX
    } else if (event.key === "-" || event.key === "_") {
      event.preventDefault();
      zoomOutCenter();
    } else if (event.key === "ArrowLeft") {
      event.preventDefault();
      const step = Math.max(1, Math.floor(visibleBp * KEY_PAN_FRACTION));
      setViewStart(Math.max(0, (pendingViewStart ?? viewStart) - step), true);
      markNavigating();
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      const step = Math.max(1, Math.floor(visibleBp * KEY_PAN_FRACTION));
      setViewStart(
        Math.min(Math.max(0, contigLength - visibleBp), (pendingViewStart ?? viewStart) + step),
        true,
      );
      markNavigating();
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
      findContigLength(seqDoc?.contigs, name) ??
      findContigLength(
        alignTracks.flatMap((t) => t.doc.contigs),
        name,
      ) ??
      0;
    visibleBp = Math.min(Math.max(MIN_VISIBLE_BP, Math.min(visibleBp, len || MIN_VISIBLE_BP)), len || MIN_VISIBLE_BP);
    selectionStart = null;
    selectionEnd = null;
    selectedFeature = null;
    selectedRead = null;
    selectedCoverage = null;
    slice = null;
    annTrackStates = annTrackStates.map((t) => ({ ...t, features: [], truncated: false, totalInRange: 0 }));
    localBuffer = null;
    alignTracks = alignTracks.map((t) => ({
      ...t,
      readCache: null,
      coverageCache: null,
      overviewBins: [],
      overviewMax: 0,
      overviewFilterKey: "",
      alignmentReads: [],
      coverageBins: [],
      coverageMax: 0,
      readPack: emptyPack(),
    }));
    clampWindow();
    if (seqDoc) void prefetchAroundViewport(true);
  }

  function coverageAtClient(clientX: number, clientY: number): CoverageBin | null {
    if (!canvasEl || !showCovTrack || visibleBp <= 0) return null;
    const rect = canvasEl.getBoundingClientRect();
    const y = clientY - rect.top;
    const layout = viewerLayout;
    const g = genomicFromClientX(clientX, canvasEl, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return null;
    for (const band of layout.covTracks) {
      if (y < band.top || y > band.top + band.height) continue;
      const track = alignTracks.find((t) => t.path === band.path);
      for (const bin of track?.coverageBins ?? []) {
        if (g >= bin.start && g < bin.end) return bin;
      }
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
    if (!readsCanvasEl || !showReadsTrack) return null;
    const rect = readsCanvasEl.getBoundingClientRect();
    const y = clientY - rect.top;
    const g = genomicFromClientX(clientX, readsCanvasEl, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return null;
    const layout = viewerLayout;
    for (const band of layout.readsTracks) {
      if (band.collapsed || band.densityOnly) continue;
      if (y < band.top || y >= band.top + band.height) continue;
      const lane = Math.floor((y - band.top - band.headerH - 2) / READ_LANE_H);
      if (lane < 0) return null;
      const track = alignTracks.find((t) => t.path === band.path);
      const hits = (track?.readPack.packed ?? []).filter(
        (p) => p.lane === lane && g >= p.read.start && g < p.read.end,
      );
      if (hits.length === 0) return null;
      hits.sort((a, b) => a.read.end - a.read.start - (b.read.end - b.read.start));
      return hits[0]!.read;
    }
    return null;
  }

  function onCanvasPointerDown(event: PointerEvent) {
    if (!hasDocument) return;
    const target = event.currentTarget as HTMLCanvasElement;
    // Always track capture so window-level safety can release it.
    try {
      target.setPointerCapture(event.pointerId);
      capturedPointerId = event.pointerId;
      captureTarget = target;
    } catch {
      capturedPointerId = null;
      captureTarget = null;
    }
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
        // Click-select only — release capture so the shell stays interactive.
        releaseCanvasPointerCapture();
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
    if (g == null) {
      releaseCanvasPointerCapture();
      return;
    }
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
      overviewCenterAt(event.clientX, target, true);
      return;
    }
    if (isPanning) {
      const layout = viewerLayout;
      const usable = Math.max(1, layout.usable);
      const dx = event.clientX - panOriginX;
      // One viewStart commit per animation frame — keeps explorer/menus responsive.
      setViewStart(panOriginStart + Math.round((-dx / usable) * visibleBp), true);
      return;
    }
    if (dragOrigin == null) return;
    const g = genomicFromClientX(event.clientX, target, viewStart, visibleBp, PAD_L, PAD_R);
    if (g == null) return;
    selectionStart = Math.min(dragOrigin, g);
    selectionEnd = Math.max(dragOrigin, g) + 1;
  }

  function onCanvasPointerUp(_event: PointerEvent) {
    endCanvasGesture();
  }

  function onCanvasLostPointerCapture() {
    if (releasingCapture) return;
    // Browser revoked capture (e.g. alert, OS gesture) — clear stuck pan state.
    capturedPointerId = null;
    captureTarget = null;
    if (isPanning || isOverviewDragging || dragOrigin != null) {
      endCanvasGesture({ fromWindow: true });
    }
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

  /** Normalize wheel delta to a rough pixel-like magnitude. */
  function wheelPixels(event: WheelEvent): number {
    if (event.deltaMode === 1) return event.deltaY * 16; // lines
    if (event.deltaMode === 2) return event.deltaY * 120; // pages
    return event.deltaY;
  }

  /** Horizontal pan: scroll down / right → move view toward higher coordinates (IGV-like). */
  function panByWheel(event: WheelEvent) {
    if (contigLength <= 0 || visibleBp <= 0) return;
    const px = wheelPixels(event);
    // Also honor horizontal trackpad swipes.
    const hPx = event.deltaMode === 0 ? event.deltaX : 0;
    const primary = Math.abs(hPx) > Math.abs(px) ? hPx : px;
    if (primary === 0) return;
    const fraction = Math.min(0.4, Math.abs(primary) / 280);
    const step = Math.max(1, Math.round(visibleBp * fraction * 0.85));
    const base = pendingViewStart ?? viewStart;
    setViewStart(base + (primary > 0 ? step : -step), true);
    // Wheel pan never set isPanning — suppress BAM/CRAM IPC until gesture ends.
    markNavigating();
  }

  /**
   * Fixed tracks (coverage, ref, features, ruler):
   *   wheel       → pan left/right
   *   Ctrl/⌘+wheel → zoom at pointer
   * Alignments track uses {@link onReadsWheel} (vertical scroll of pileup).
   */
  function onFixedTracksWheel(event: WheelEvent) {
    event.preventDefault();
    if (event.ctrlKey || event.metaKey) {
      zoomAtClientX(event.clientX, wheelPixels(event) > 0 ? ZOOM_OUT_FACTOR : ZOOM_IN_FACTOR);
      return;
    }
    panByWheel(event);
  }

  /**
   * Alignments pileup:
   *   wheel       → scroll the read list vertically
   *   Ctrl/⌘+wheel → zoom at pointer (same as coverage)
   *   Shift+wheel → pan left/right without leaving the track
   */
  function onReadsWheel(event: WheelEvent) {
    if (event.ctrlKey || event.metaKey) {
      event.preventDefault();
      zoomAtClientX(event.clientX, wheelPixels(event) > 0 ? ZOOM_OUT_FACTOR : ZOOM_IN_FACTOR);
      return;
    }
    if (event.shiftKey) {
      event.preventDefault();
      panByWheel(event);
      return;
    }
    // Canvas sits inside `.reads-scroll` — drive vertical scroll explicitly so
    // the pileup always moves (browser default is unreliable over <canvas>).
    const scrollEl = (event.currentTarget as HTMLElement | null)?.closest(
      ".reads-scroll",
    ) as HTMLElement | null;
    if (!scrollEl) return;
    const px = wheelPixels(event);
    if (px === 0) return;
    const maxScroll = scrollEl.scrollHeight - scrollEl.clientHeight;
    if (maxScroll <= 0) return;
    event.preventDefault();
    scrollEl.scrollTop = Math.max(0, Math.min(maxScroll, scrollEl.scrollTop + px));
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
              <label class="pop-check"><input type="checkbox" bind:checked={colorBases} /> Letters on mismatch bases (reads)</label>
            {/if}
            {#if alignDoc}
              <label class="pop-check" title="Optional whole-contig scan; leave off for fastest local navigation"><input type="checkbox" bind:checked={showOverviewDepth} /> Whole-contig depth overview</label>
              <label class="pop-check"><input type="checkbox" bind:checked={colorMismatches} /> Mismatch &amp; indel highlights</label>
            {/if}
            <p class="pop-hint">Use ▾ / ▸ in the track gutter to collapse or expand tracks. Coverage is depth (grey) only.</p>
            {#if annDocs.length > 0}
              <label class="pop-field">
                <span>Features</span>
                <select bind:value={featureTypeFilter}>
                  <option value="genes">Genes / transcripts</option>
                  <option value="exons">Exons / UTRs</option>
                  <option value="cds">CDS only</option>
                  <option value="all">All types</option>
                </select>
              </label>
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
                  <option value="none">Gray</option>
                  <option value="strand">Strand</option>
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
            {#if !String(band.id).startsWith("reads")}
              <div
                class="gutter-row"
                class:collapsed={band.collapsed}
                style:height="{band.height}px"
                title={band.label}
              >
                {#if band.id === "seq"}
                  <button
                    type="button"
                    class="gutter-collapse"
                    class:collapsed={!showSeqTrackVisible}
                    title={showSeqTrackVisible ? "Collapse reference" : "Expand reference"}
                    aria-label={showSeqTrackVisible ? "Collapse reference" : "Expand reference"}
                    aria-expanded={showSeqTrackVisible}
                    onclick={() => (showSeqTrackVisible = !showSeqTrackVisible)}
                  >{showSeqTrackVisible ? "▾" : "▸"}</button>
                {:else if String(band.id).startsWith("cov")}
                  <button
                    type="button"
                    class="gutter-collapse"
                    class:collapsed={!showCoverage}
                    title={showCoverage ? "Collapse coverage" : "Expand coverage"}
                    aria-label={showCoverage ? "Collapse coverage" : "Expand coverage"}
                    aria-expanded={showCoverage}
                    disabled={alignTracks.length === 0}
                    onclick={() => (showCoverage = !showCoverage)}
                  >{showCoverage ? "▾" : "▸"}</button>
                {:else if band.id.startsWith("ann:")}
                  {@const annMeta = viewerLayout.annTracks.find((t) => t.top === band.top)}
                  {@const annExpanded =
                    annTrackStates.find((t) => t.path === annMeta?.path)?.visible ?? true}
                  <button
                    type="button"
                    class="gutter-collapse"
                    class:collapsed={!annExpanded}
                    title={annExpanded ? `Collapse ${band.label}` : `Expand ${band.label}`}
                    aria-label={annExpanded ? `Collapse ${band.label}` : `Expand ${band.label}`}
                    aria-expanded={annExpanded}
                    onclick={() => annMeta && setAnnTrackVisible(annMeta.path, !annExpanded)}
                  >{annExpanded ? "▾" : "▸"}</button>
                {:else}
                  <span class="gutter-spacer"></span>
                {/if}
                <span class="gutter-label">{band.label}</span>
              </div>
            {/if}
          {/each}
          {#each viewerLayout.readsTracks as rd (rd.id)}
            <div
              class="gutter-row gutter-reads"
              class:collapsed={rd.collapsed}
              style:height={rd.collapsed ? `${rd.height}px` : undefined}
            >
              <button
                type="button"
                class="gutter-collapse"
                class:collapsed={!showReadPileup}
                title={showReadPileup ? "Collapse alignments" : "Expand alignments"}
                aria-label={showReadPileup ? "Collapse alignments" : "Expand alignments"}
                aria-expanded={showReadPileup}
                disabled={alignTracks.length === 0}
                onclick={() => (showReadPileup = !showReadPileup)}
              >{showReadPileup ? "▾" : "▸"}</button>
              <span class="gutter-label">{rd.label}</span>
            </div>
          {/each}
        </div>
        <div class="track-plots" bind:this={plotsEl}>
          <div class="fixed-tracks">
            <canvas
              bind:this={canvasEl}
              class="view-canvas"
              class:panning={isPanning || isOverviewDragging}
              onpointerdown={onCanvasPointerDown}
              onpointermove={onCanvasPointerMove}
              onpointerup={onCanvasPointerUp}
              onpointercancel={onCanvasPointerUp}
              onlostpointercapture={onCanvasLostPointerCapture}
              ondblclick={onCanvasDoubleClick}
              onwheel={onFixedTracksWheel}
            ></canvas>
          </div>
          {#if readsAvailable}
            <div
              class="reads-scroll"
              class:collapsed={viewerLayout.readsTracks.every((t) => t.collapsed)}
              style:min-height={viewerLayout.readsTracks.every((t) => t.collapsed) ? "0" : undefined}
              style:flex={viewerLayout.readsTracks.every((t) => t.collapsed) ? "0 0 auto" : undefined}
            >
              <canvas
                bind:this={readsCanvasEl}
                class="view-canvas"
                class:panning={isPanning}
                onpointerdown={onCanvasPointerDown}
                onpointermove={onCanvasPointerMove}
                onpointerup={onCanvasPointerUp}
                onpointercancel={onCanvasPointerUp}
                onlostpointercapture={onCanvasLostPointerCapture}
                ondblclick={onCanvasDoubleClick}
                onwheel={onReadsWheel}
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
            · CIGAR {selectedRead.cigar || "—"}
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
            Drag to pan · Shift+drag select · wheel pan · Ctrl+wheel zoom · scroll alignments · +/− keys
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
    flex: 1 1 auto;
    height: 100%;
    width: 100%;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 8px;
    flex-wrap: wrap;
    flex: 0 0 auto;
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

  .pop-hint {
    margin: 4px 6px 2px;
    font-size: 0.7rem;
    color: var(--text-faint);
    line-height: 1.35;
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
    transition: background 0.12s ease, border-color 0.12s ease, color 0.12s ease;
  }

  .ghost:hover:not(:disabled) {
    background: var(--menu-hover-bg);
    border-color: var(--chip-active-border);
  }

  .primary {
    background: var(--primary-bg);
    color: var(--primary-text);
    box-shadow: var(--primary-shadow);
    transition: filter 0.12s ease, transform 0.12s ease;
  }

  .primary:hover:not(:disabled) {
    filter: brightness(1.05);
  }

  .primary:active:not(:disabled) {
    transform: translateY(0.5px);
  }

  .primary:disabled,
  .ghost:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .viewer {
    flex: 1 1 auto;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    border-radius: 8px;
    border: 1px solid var(--view-track-border, var(--panel-border));
    overflow: hidden;
    background: var(--view-canvas-bg, #f7f7f7);
    outline: none;
    box-shadow: 0 1px 0 rgba(255, 255, 255, 0.03) inset;
  }

  .viewer:focus-visible {
    border-color: var(--chip-active-border);
    box-shadow: 0 0 0 2px var(--focus-ring, rgba(53, 206, 231, 0.25));
  }

  .viewer-main {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: row;
    width: 100%;
  }

  .track-gutter {
    flex: 0 0 auto;
    width: 108px;
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

  .gutter-row.gutter-reads.collapsed {
    flex: 0 0 auto;
    min-height: 0;
    align-items: center;
    padding-top: 0;
  }

  .gutter-row.collapsed {
    opacity: 0.85;
  }

  .gutter-collapse {
    flex: 0 0 14px;
    width: 14px;
    height: 14px;
    margin: 0;
    padding: 0;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
    font-size: 0.72rem;
    line-height: 1;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .gutter-collapse:hover:not(:disabled) {
    color: var(--text-primary);
    background: var(--menu-hover-bg);
  }

  .gutter-collapse:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }

  .gutter-collapse.collapsed {
    color: var(--accent-highlight, var(--text-menu));
  }

  .gutter-spacer {
    width: 14px;
    flex: 0 0 14px;
  }

  .gutter-label {
    font-size: 0.68rem;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.2;
  }

  .reads-scroll.collapsed {
    flex: 0 0 auto;
    min-height: 0;
    overflow: hidden;
  }

  .track-plots {
    flex: 1 1 auto;
    min-width: 0;
    width: 100%;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .fixed-tracks {
    flex: 0 0 auto;
    width: 100%;
  }

  .reads-scroll {
    flex: 1 1 auto;
    min-height: 100px;
    overflow-x: hidden;
    overflow-y: auto;
    border-top: 1px solid var(--view-track-border, var(--panel-border));
    width: 100%;
  }

  .view-canvas {
    display: block;
    width: 100%;
    max-width: 100%;
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
