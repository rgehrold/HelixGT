import {
  COV_CACHE_PAD_BP,
  COV_CACHE_VIEWPORTS,
  COV_FETCH_CHUNK_MAX_BP,
  MAX_COVERAGE_BINS,
  READ_CACHE_PAD_BP,
  READ_FETCH_CHUNK_MAX_BP,
  VIEW_SIDE_PAD_FRACTION,
} from "./constants";

/**
 * Coverage resolution: always ~one bin per screen pixel (cheap histogram).
 * Engine uses a difference-array scan — O(reads + bins), not per-base.
 */
export function desiredCoverageBinCount(windowSpanBp: number, plotWidth?: number, visibleBp = windowSpanBp): number {
  const px = Math.max(40, Math.floor(plotWidth ?? 640));
  const paddedPixels = Math.ceil(px * windowSpanBp / Math.max(1, visibleBp));
  return Math.max(1, Math.min(MAX_COVERAGE_BINS, windowSpanBp, paddedPixels));
}

/**
 * Max depth among bins overlapping [viewStart, viewEnd).
 * Used for Y-scale so an off-screen spike in a padded cache does not squash
 * the visible coverage track (e.g. uniform 38× looking empty under a 500× max).
 */
export function maxDepthInView(
  bins: { start: number; end: number; depth: number }[],
  viewStart: number,
  viewEnd: number,
): number {
  let m = 0;
  for (const b of bins) {
    if (b.end <= viewStart) continue;
    if (b.start >= viewEnd) break;
    if (b.depth > m) m = b.depth;
  }
  return m;
}

/** Pad each side of the viewport (≥ fraction of visible span, with absolute floors). */
function sidePad(visibleBp: number, absFloor: number): number {
  return Math.max(absFloor, Math.floor(visibleBp * VIEW_SIDE_PAD_FRACTION));
}

/** Genomic window to request for alignment reads around the viewport. */
export function computeReadFetchWindow(
  viewStart: number,
  viewEnd: number,
  contigLength: number,
  visibleBp: number,
): { start: number; end: number } {
  // Tight pad: pileup is only fetched when zoomed in; keep scan small.
  const pad = Math.min(
    READ_CACHE_PAD_BP,
    Math.max(1_000, Math.floor(visibleBp * VIEW_SIDE_PAD_FRACTION)),
  );
  let start = Math.max(0, viewStart - pad);
  let end = contigLength > 0 ? Math.min(contigLength, viewEnd + pad) : viewEnd + pad;

  const maxSpan = Math.min(READ_FETCH_CHUNK_MAX_BP, Math.max(visibleBp + pad * 2, 4_000));
  if (end - start > maxSpan) {
    const center = viewStart + visibleBp / 2;
    start = Math.max(0, Math.floor(center - maxSpan / 2));
    end = contigLength > 0 ? Math.min(contigLength, start + maxSpan) : start + maxSpan;
    start = Math.max(0, end - maxSpan);
  }

  return { start, end };
}

/**
 * Wide padded window for coverage fetch.
 * Bins are ~1 per screen pixel regardless of span — a multi-viewport cache is
 * cheap over the wire and makes pan at 50 kb free until you leave the cache.
 */
export function computeCoverageFetchWindow(
  viewStart: number,
  viewEnd: number,
  contigLength: number,
  visibleBp: number,
): { start: number; end: number } {
  // A fetch cap must not trim away the viewport itself at chromosome scale.
  if (viewEnd - viewStart >= COV_FETCH_CHUNK_MAX_BP) {
    return { start: viewStart, end: contigLength > 0 ? Math.min(contigLength, viewEnd) : viewEnd };
  }
  const vp = Math.max(1, COV_CACHE_VIEWPORTS);
  // Half the extra viewports on each side, plus an absolute floor for short windows.
  const padFromViewports = Math.floor((visibleBp * (vp - 1)) / 2);
  const pad = Math.max(
    Math.min(COV_CACHE_PAD_BP, Math.max(1, Math.floor(visibleBp * 0.5))),
    padFromViewports,
  );
  let start = Math.max(0, viewStart - pad);
  let end = contigLength > 0 ? Math.min(contigLength, viewEnd + pad) : viewEnd + pad;

  const maxSpan = Math.min(
    COV_FETCH_CHUNK_MAX_BP,
    Math.max(visibleBp * vp, visibleBp + pad * 2, 80_000),
  );
  if (end - start > maxSpan) {
    const center = viewStart + visibleBp / 2;
    start = Math.max(0, Math.floor(center - maxSpan / 2));
    end = contigLength > 0 ? Math.min(contigLength, start + maxSpan) : start + maxSpan;
    start = Math.max(0, end - maxSpan);
  }
  return { start, end };
}

export function cacheCoversWindow(
  cache: { start: number; end: number; contig: string; path: string; filterKey: string } | null,
  path: string,
  contig: string,
  filterKey: string,
  viewStart: number,
  viewEnd: number,
): boolean {
  if (!cache) return false;
  return (
    cache.path === path &&
    cache.contig === contig &&
    cache.filterKey === filterKey &&
    viewStart >= cache.start &&
    viewEnd <= cache.end
  );
}

/** Strip `|seq` / `|noseq` so bar-level and sequence-level caches share filter identity. */
function readFilterBaseKey(filterKey: string): string {
  return filterKey.replace(/\|(seq|noseq)$/i, "");
}

/**
 * Whether cached reads can be *painted* for the current viewport.
 * Accepts both seq and no-seq samples with the same MAPQ/flag filters, and still
 * serves truncated samples so zoom-in never blanks the pileup while a re-fetch runs.
 */
export function readCacheCanDisplay(
  cache: {
    start: number;
    end: number;
    contig: string;
    path: string;
    filterKey: string;
  } | null,
  path: string,
  contig: string,
  filterKey: string,
  viewStart: number,
  viewEnd: number,
): boolean {
  if (!cache) return false;
  if (cache.path !== path || cache.contig !== contig) return false;
  if (readFilterBaseKey(cache.filterKey) !== readFilterBaseKey(filterKey)) return false;
  return viewStart >= cache.start && viewEnd <= cache.end;
}

/**
 * Whether the cache fully satisfies the viewport (skip network).
 * Truncated wide-window samples must re-query when zoomed in — sparse loci on
 * the right of a dense region may be missing from the sample.
 */
export function readCacheUsable(
  cache: {
    start: number;
    end: number;
    contig: string;
    path: string;
    filterKey: string;
    truncated: boolean;
    hasSequences: boolean;
  } | null,
  path: string,
  contig: string,
  filterKey: string,
  viewStart: number,
  viewEnd: number,
  needSequences: boolean,
): boolean {
  if (!readCacheCanDisplay(cache, path, contig, filterKey, viewStart, viewEnd)) {
    return false;
  }
  if (!cache) return false;
  // Prefer exact seq/noseq key when present; allow display-compat cache only via CanDisplay.
  if (cache.filterKey !== filterKey) {
    // e.g. have |noseq but want |seq → must refresh
    if (needSequences && !cache.hasSequences) return false;
    if (!needSequences && cache.filterKey.endsWith("|seq")) {
      // seq cache is a superset for drawing; OK to skip re-fetch when zoomed out
    } else if (needSequences && cache.hasSequences) {
      // has sequences under a compatible base filter — OK
    } else {
      return false;
    }
  }
  if (needSequences && !cache.hasSequences) return false;
  // Do NOT reject every truncated mid-zoom cache (visible < 45% of span).
  // Dense WGS regions are almost always truncated → that rule re-fetched on
  // every pan and froze the UI. Only force a denser re-query when zoomed far
  // into a huge truncated sample.
  if (cache.truncated) {
    const cacheSpan = Math.max(1, cache.end - cache.start);
    const visible = Math.max(1, viewEnd - viewStart);
    if (visible <= 2_000 && cacheSpan > visible * 20) return false;
  }
  return true;
}

export function cacheNearEdge(
  cache: { start: number; end: number },
  viewStart: number,
  viewEnd: number,
  visibleBp: number,
): boolean {
  const margin = Math.max(
    2_000,
    Math.min(READ_CACHE_PAD_BP / 2, Math.floor(visibleBp * VIEW_SIDE_PAD_FRACTION * 0.5)),
  );
  return viewStart - cache.start < margin || cache.end - viewEnd < margin;
}

/**
 * Coverage cache is usable if it covers the genomic window.
 * Do not thrash on binCount / density — that caused re-fetch on every resize/zoom.
 */
export function coverageCacheCovers(
  cache: { start: number; end: number; contig: string; path: string; binCount: number } | null,
  path: string,
  contig: string,
  viewStart: number,
  viewEnd: number,
  _maxBpPerBin?: number,
): boolean {
  if (!cache) return false;
  if (cache.path !== path || cache.contig !== contig) return false;
  return viewStart >= cache.start && viewEnd <= cache.end;
}

/** True when cached bins are fine enough for the current viewport. */
export function coverageCacheDenseEnough(
  cache: { start: number; end: number; binCount: number } | null,
  viewStart: number,
  viewEnd: number,
  plotWidth: number,
): boolean {
  if (!cache) return false;
  const cacheSpan = Math.max(1, cache.end - cache.start);
  const bpPerBin = cacheSpan / Math.max(1, cache.binCount);
  const viewSpan = Math.max(1, viewEnd - viewStart);
  const targetBpPerBin = viewSpan / Math.max(40, plotWidth);
  // Allow some coarseness so we don't refetch on every zoom tick.
  return bpPerBin <= Math.max(1.5, targetBpPerBin * 2.5);
}

export function coverageNearEdge(
  cache: { start: number; end: number },
  viewStart: number,
  viewEnd: number,
  visibleBp: number,
): boolean {
  const margin = Math.max(
    4_000,
    Math.min(COV_CACHE_PAD_BP / 2, Math.floor(visibleBp * VIEW_SIDE_PAD_FRACTION * 0.5)),
  );
  return viewStart - cache.start < margin || cache.end - viewEnd < margin;
}
