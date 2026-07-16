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
  let hiddenCount = 0;

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

export function readsOverlapWindow(
  reads: AlignmentRead[],
  start: number,
  end: number,
): AlignmentRead[] {
  if (reads.length === 0) return [];
  // Reads from the engine are sorted by start, then name.
  let lo = 0;
  let hi = reads.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (reads[mid]!.end <= start) lo = mid + 1;
    else hi = mid;
  }
  const out: AlignmentRead[] = [];
  for (let i = lo; i < reads.length; i++) {
    const r = reads[i]!;
    if (r.start >= end) break;
    if (r.end > start) out.push(r);
  }
  return out;
}

function matchesMatch(op: string): boolean {
  return op === "M" || op === "=" || op === "X";
}

/**
 * Per-base mismatch fraction in the viewport for optional coverage tinting.
 * Index i = genomic position viewStart + i.
 */
export function mismatchFractions(
  reads: AlignmentRead[],
  viewStart: number,
  viewEnd: number,
  ref: { start: number; end: number; sequence: string } | null,
  maxBp: number,
): Float32Array | null {
  if (!ref || reads.length === 0 || viewEnd <= viewStart) return null;
  const span = viewEnd - viewStart;
  if (span > maxBp) return null;
  const n = span;
  const counts = new Float32Array(n);
  const depth = new Float32Array(n);

  for (const r of reads) {
    if (r.end <= viewStart || r.start >= viewEnd || !r.sequence) continue;
    let refPos = r.start;
    let seqPos = 0;
    const ops =
      r.cigarOps?.length > 0
        ? r.cigarOps
        : [{ op: "M", length: Math.max(1, r.end - r.start) }];
    for (const op of ops) {
      const opChar = op.op;
      const len = op.length;
      if (opChar === "S" || opChar === "H") {
        if (opChar === "S") seqPos += len;
        continue;
      }
      if (opChar === "I" || opChar === "P") {
        if (opChar === "I") seqPos += len;
        continue;
      }
      if (opChar === "D" || opChar === "N") {
        refPos += len;
        continue;
      }
      if (matchesMatch(opChar)) {
        for (let k = 0; k < len; k++) {
          const p = refPos + k;
          if (p < viewStart || p >= viewEnd) continue;
          const i = p - viewStart;
          depth[i]! += 1;
          const base = r.sequence[seqPos + k] ?? "";
          if (p >= ref.start && p < ref.end) {
            const rb = ref.sequence[p - ref.start] ?? "";
            if (base && rb && base.toUpperCase() !== rb.toUpperCase()) {
              counts[i]! += 1;
            }
          }
        }
        refPos += len;
        seqPos += len;
      }
    }
  }

  const frac = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    frac[i] = depth[i]! > 0 ? counts[i]! / depth[i]! : 0;
  }
  return frac;
}