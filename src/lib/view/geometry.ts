import {
  ANN_BAND_PAD,
  COV_BAND_H,
  FEATURE_LANE_H,
  MAX_FEATURE_LANES,
  OVERVIEW_H,
  PAD_L,
  PAD_R,
  READ_LANE_H,
  READS_HEADER_H,
  RULER_H,
  SEQ_BAND_H,
} from "./constants";
import type { AnnotationFeature } from "$lib/types";

export type TrackBand = {
  id: string;
  label: string;
  top: number;
  height: number;
  toggleable: boolean;
};

export type AnnTrackLayout = TrackBand & {
  path: string;
  laneCount: number;
};

export type ReadsTrackLayout = {
  headerH: number;
  laneH: number;
  laneCount: number;
  height: number;
};

export type UnifiedLayout = {
  padL: number;
  padR: number;
  usable: number;
  overview: TrackBand;
  ruler: TrackBand;
  seq: TrackBand | null;
  cov: TrackBand | null;
  annTracks: AnnTrackLayout[];
  reads: ReadsTrackLayout | null;
  fixedHeight: number;
  bands: TrackBand[];
};

export type VisibleTracks = {
  showSeq: boolean;
  showCov: boolean;
  annTracks: { path: string; label: string; visible: boolean; laneCount: number }[];
  showReads: boolean;
  readLaneCount: number;
};

export function estimateFeatureLanes(
  features: AnnotationFeature[],
  viewStart: number,
  viewEnd: number,
): number {
  if (features.length === 0) return 1;
  const ends: number[] = [];
  for (const f of features) {
    if (f.end <= viewStart || f.start >= viewEnd) continue;
    let lane = 0;
    while (lane < ends.length && ends[lane]! > f.start) lane++;
    if (lane === ends.length) ends.push(0);
    ends[lane] = f.end;
    if (ends.length >= MAX_FEATURE_LANES) break;
  }
  return Math.max(1, ends.length);
}

export function computeUnifiedLayout(cssW: number, tracks: VisibleTracks): UnifiedLayout {
  const padL = PAD_L;
  const padR = PAD_R;
  const usable = Math.max(1, cssW - padL - padR);
  const bands: TrackBand[] = [];
  let y = 0;

  const overview: TrackBand = {
    id: "overview",
    label: "Overview",
    top: y,
    height: OVERVIEW_H,
    toggleable: false,
  };
  bands.push(overview);
  y += OVERVIEW_H + 1;

  const ruler: TrackBand = {
    id: "ruler",
    label: "Scale",
    top: y,
    height: RULER_H,
    toggleable: false,
  };
  bands.push(ruler);
  y += RULER_H + 1;

  let seq: TrackBand | null = null;
  if (tracks.showSeq) {
    seq = { id: "seq", label: "Ref", top: y, height: SEQ_BAND_H, toggleable: true };
    bands.push(seq);
    y += SEQ_BAND_H + 1;
  }

  let cov: TrackBand | null = null;
  if (tracks.showCov) {
    cov = { id: "cov", label: "Coverage", top: y, height: COV_BAND_H, toggleable: true };
    bands.push(cov);
    y += COV_BAND_H + 1;
  }

  const annTracks: AnnTrackLayout[] = [];
  for (const ann of tracks.annTracks) {
    if (!ann.visible) continue;
    const lanes = Math.min(MAX_FEATURE_LANES, Math.max(1, ann.laneCount));
    const h = ANN_BAND_PAD + lanes * FEATURE_LANE_H;
    const band: AnnTrackLayout = {
      id: `ann:${ann.path}`,
      path: ann.path,
      label: ann.label,
      top: y,
      height: h,
      toggleable: true,
      laneCount: lanes,
    };
    annTracks.push(band);
    bands.push(band);
    y += h + 1;
  }

  let reads: ReadsTrackLayout | null = null;
  if (tracks.showReads) {
    const laneCount = Math.max(1, tracks.readLaneCount);
    const height = READS_HEADER_H + laneCount * READ_LANE_H + 4;
    reads = { headerH: READS_HEADER_H, laneH: READ_LANE_H, laneCount, height };
    bands.push({
      id: "reads",
      label: "Alignments",
      top: y,
      height,
      toggleable: true,
    });
  }

  return {
    padL,
    padR,
    usable,
    overview,
    ruler,
    seq,
    cov,
    annTracks,
    reads,
    fixedHeight: y,
    bands,
  };
}

/** @deprecated */
export function computeTrackLayout(
  cssW: number,
  tracks: {
    showSeq: boolean;
    showCov: boolean;
    showFeat: boolean;
    showReads: boolean;
  },
  featureLaneCount: number,
) {
  return computeUnifiedLayout(cssW, {
    showSeq: tracks.showSeq,
    showCov: tracks.showCov,
    annTracks: tracks.showFeat
      ? [{ path: "_", label: "Features", visible: true, laneCount: featureLaneCount }]
      : [],
    showReads: tracks.showReads,
    readLaneCount: 1,
  });
}

/** Map client X on a canvas element to 0-based genomic coordinate. */
export function genomicFromClientX(
  clientX: number,
  el: HTMLElement,
  viewStart: number,
  visibleBp: number,
  padL = PAD_L,
  padR = PAD_R,
): number | null {
  if (visibleBp <= 0) return null;
  const rect = el.getBoundingClientRect();
  const x = clientX - rect.left;
  const usable = Math.max(1, rect.width - padL - padR);
  const t = Math.max(0, Math.min(1, (x - padL) / usable));
  return Math.floor(viewStart + t * visibleBp);
}

export function xForGenomic(
  genomic: number,
  viewStart: number,
  visibleBp: number,
  padL: number,
  usable: number,
): number {
  if (visibleBp <= 0) return padL;
  return padL + ((genomic - viewStart) / visibleBp) * usable;
}

export function formatGenomicPos(pos1Based: number): string {
  if (pos1Based >= 1_000_000) return `${(pos1Based / 1_000_000).toFixed(2)}M`;
  if (pos1Based >= 10_000) return `${(pos1Based / 1_000).toFixed(1)}k`;
  return String(pos1Based);
}

export type OverviewHit = "thumb" | "track" | null;

export function hitOverview(
  clientY: number,
  el: HTMLElement,
  layout: UnifiedLayout,
): OverviewHit {
  const rect = el.getBoundingClientRect();
  const y = clientY - rect.top;
  if (y < layout.overview.top || y > layout.overview.top + layout.overview.height) return null;
  return "track";
}

export function bandAtClientY(
  clientY: number,
  el: HTMLElement,
  layout: UnifiedLayout,
): TrackBand | AnnTrackLayout | null {
  const rect = el.getBoundingClientRect();
  const y = clientY - rect.top;
  for (const band of layout.bands) {
    if (y >= band.top && y < band.top + band.height) return band;
  }
  return null;
}

export function defaultVisibleBp(contigLength: number): number {
  if (contigLength <= 0) return 500;
  if (contigLength <= 50_000) return contigLength;
  return Math.min(50_000, contigLength);
}