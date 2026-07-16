import {
  COV_CACHE_PAD_BP,
  COV_FETCH_CHUNK_MAX_BP,
  READ_CACHE_PAD_BP,
  READ_FETCH_CHUNK_MAX_BP,
} from "./constants";

/** Genomic window to request for alignment reads around the viewport. */
export function computeReadFetchWindow(
  viewStart: number,
  viewEnd: number,
  contigLength: number,
  visibleBp: number,
): { start: number; end: number } {
  const pad = Math.min(READ_CACHE_PAD_BP, Math.max(4_000, Math.floor(visibleBp * 0.6)));
  let start = Math.max(0, viewStart - pad);
  let end = contigLength > 0 ? Math.min(contigLength, viewEnd + pad) : viewEnd + pad;

  const maxSpan = Math.min(
    READ_FETCH_CHUNK_MAX_BP,
    Math.max(visibleBp + pad * 2, 12_000),
  );
  if (end - start > maxSpan) {
    const center = viewStart + visibleBp / 2;
    start = Math.max(0, Math.floor(center - maxSpan / 2));
    end = contigLength > 0 ? Math.min(contigLength, start + maxSpan) : start + maxSpan;
    start = Math.max(0, end - maxSpan);
  }

  return { start, end };
}

/** Wider padded window for coverage (cheap enough after native BAM). */
export function computeCoverageFetchWindow(
  viewStart: number,
  viewEnd: number,
  contigLength: number,
  visibleBp: number,
): { start: number; end: number } {
  const pad = Math.min(COV_CACHE_PAD_BP, Math.max(8_000, Math.floor(visibleBp * 1.2)));
  let start = Math.max(0, viewStart - pad);
  let end = contigLength > 0 ? Math.min(contigLength, viewEnd + pad) : viewEnd + pad;

  const maxSpan = Math.min(
    COV_FETCH_CHUNK_MAX_BP,
    Math.max(visibleBp + pad * 2, 20_000),
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

export function cacheNearEdge(
  cache: { start: number; end: number },
  viewStart: number,
  viewEnd: number,
  visibleBp: number,
): boolean {
  const margin = Math.min(READ_CACHE_PAD_BP / 2, Math.max(2_000, Math.floor(visibleBp * 0.25)));
  return viewStart - cache.start < margin || cache.end - viewEnd < margin;
}

export function coverageCacheCovers(
  cache: { start: number; end: number; contig: string; path: string; binCount: number } | null,
  path: string,
  contig: string,
  viewStart: number,
  viewEnd: number,
  binCount: number,
): boolean {
  if (!cache) return false;
  // Allow small bin-count drift so pan doesn't thrash on width resize noise.
  if (Math.abs(cache.binCount - binCount) > Math.max(20, binCount * 0.25)) return false;
  return (
    cache.path === path &&
    cache.contig === contig &&
    viewStart >= cache.start &&
    viewEnd <= cache.end
  );
}

export function coverageNearEdge(
  cache: { start: number; end: number },
  viewStart: number,
  viewEnd: number,
  visibleBp: number,
): boolean {
  const margin = Math.min(COV_CACHE_PAD_BP / 2, Math.max(4_000, Math.floor(visibleBp * 0.3)));
  return viewStart - cache.start < margin || cache.end - viewEnd < margin;
}
