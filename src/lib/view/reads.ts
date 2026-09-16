import type { AlignmentRead } from "$lib/types";
import { MAX_READ_LANES } from "./constants";

export type PackedRead = { read: AlignmentRead; lane: number };

export type PackReadsResult = {
  packed: PackedRead[];
  laneCount: number;
  hiddenCount: number;
};

/**
 * IGV-style squish packing: assign each read to the first lane whose
 * previous occupant ends before this read starts.
 * Reads from the engine are already sorted by start — we only re-sort
 * if a small unsorted sample is detected (stable for cache slices).
 */
export function packReadsSquish(
  reads: AlignmentRead[],
  viewStart: number,
  viewEnd: number,
  maxLanes = MAX_READ_LANES,
): PackReadsResult {
  // Binary filter of overlapping reads (engine output is start-sorted).
  let visible = readsOverlapWindow(reads, viewStart, viewEnd);
  // Hard cap inputs before packing — packing O(n×lanes) gets expensive.
  const PACK_INPUT_CAP = 6_000;
  const omittedByCap = Math.max(0, visible.length - PACK_INPUT_CAP);
  if (visible.length > PACK_INPUT_CAP) {
    visible = visible.slice(0, PACK_INPUT_CAP);
  }
  if (visible.length > 1) {
    let needsSort = false;
    for (let i = 1; i < Math.min(visible.length, 32); i++) {
      if (visible[i]!.start < visible[i - 1]!.start) {
        needsSort = true;
        break;
      }
    }
    if (needsSort) {
      visible = [...visible].sort(
        (a, b) => a.start - b.start || a.end - b.end || a.name.localeCompare(b.name),
      );
    }
  }

  const laneEnds: number[] = [];
  const packed: PackedRead[] = [];
  let hiddenCount = omittedByCap;

  for (const read of visible) {
    let lane = 0;
    while (lane < laneEnds.length && laneEnds[lane]! > read.start) lane++;
    if (lane >= maxLanes) {
      hiddenCount++;
      continue;
    }
    if (lane === laneEnds.length) laneEnds.push(0);
    // Small gap so adjacent reads don't visually touch when zoomed out.
    laneEnds[lane] = read.end + 1;
    packed.push({ read, lane });
  }

  return { packed, laneCount: Math.max(1, laneEnds.length), hiddenCount };
}

/** @deprecated Use packReadsSquish */
export function packReadsLongestFirst(
  reads: AlignmentRead[],
  viewStart: number,
  viewEnd: number,
): PackedRead[] {
  return packReadsSquish(reads, viewStart, viewEnd).packed;
}

export function readKey(r: AlignmentRead): string {
  return `${r.name}|${r.start}|${r.flags}`;
}

export function flagSummary(r: AlignmentRead): string {
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

export function meanQuality(r: AlignmentRead): string {
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

export function readFilterKey(opts: {
  hideSecondary: boolean;
  hideSupplementary: boolean;
  hideDuplicates: boolean;
  minMapq: number;
}): string {
  return [
    opts.hideSecondary ? "1" : "0",
    opts.hideSupplementary ? "1" : "0",
    opts.hideDuplicates ? "1" : "0",
    String(opts.minMapq),
  ].join("|");
}

/**
 * Filter start-sorted reads that overlap [start, end) (half-open).
 *
 * Important: we must NOT binary-search by `read.end` on a start-sorted list.
 * A long read that starts before the viewport (and covers it) can be followed by
 * short reads that end before the viewport — searching by end would skip the
 * long read and make the pileup empty when zoomed in.
 */
export function readsOverlapWindow(
  reads: AlignmentRead[],
  start: number,
  end: number,
): AlignmentRead[] {
  if (reads.length === 0 || end <= start) return [];

  // Upper bound: first index with start >= end (everything after is past the window).
  let hi = reads.length;
  {
    let lo = 0;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (reads[mid]!.start < end) lo = mid + 1;
      else hi = mid;
    }
  }

  // Scan [0, hi). Early-starting reads that end before `start` are skipped;
  // those that still cover the window are kept. Cap is small (engine max ~50k).
  const out: AlignmentRead[] = [];
  for (let i = 0; i < hi; i++) {
    const r = reads[i]!;
    if (r.end > start) out.push(r);
  }
  return out;
}
