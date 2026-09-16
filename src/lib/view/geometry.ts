import {
  ANN_BAND_PAD,
  COLLAPSED_TRACK_H,
  COV_BAND_H,
  FEATURE_LANE_H,
  MAX_FEATURE_LANES,
  DENSITY_STRIP_H,
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
  /** When true, only a thin header is shown (collapsed via gutter arrow). */
  collapsed?: boolean;
};

export type AnnTrackLayout = TrackBand & {
  path: string;
  laneCount: number;
};

export type CovTrackLayout = TrackBand & {
  path: string;
};

export type ReadsTrackLayout = {
  id: string;
  path: string;
  label: string;
  top: number;
  headerH: number;
  laneH: number;
  laneCount: number;
  height: number;
  collapsed: boolean;
  densityOnly: boolean;
};

export type UnifiedLayout = {
  padL: number;
  padR: number;
  usable: number;
  overview: TrackBand;
  ruler: TrackBand;
  seq: TrackBand | null;
  /** @deprecated first coverage track — prefer covTracks */
  cov: CovTrackLayout | null;
  covTracks: CovTrackLayout[];
  annTracks: AnnTrackLayout[];
  /** @deprecated first reads track — prefer readsTracks */
  reads: ReadsTrackLayout | null;
  readsTracks: ReadsTrackLayout[];
  fixedHeight: number;
  readsHeight: number;
  bands: TrackBand[];
};

export type VisibleTracks = {
  seqAvailable: boolean;
  seqExpanded: boolean;
  covTracks: { path: string; label: string; expanded: boolean }[];
  annTracks: { path: string; label: string; expanded: boolean; laneCount: number }[];
  readsTracks: {
    path: string;
    label: string;
    expanded: boolean;
    laneCount: number;
    densityOnly: boolean;
  }[];
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

  // Track order (IGV-like): overview → scale → coverage(s) → reference → features → reads.
  const covTracks: CovTrackLayout[] = [];
  for (const covIn of tracks.covTracks) {
    const collapsed = !covIn.expanded;
    const height = collapsed ? COLLAPSED_TRACK_H : COV_BAND_H;
    const multi = tracks.covTracks.length > 1;
    const band: CovTrackLayout = {
      id: `cov:${covIn.path}`,
      path: covIn.path,
      label: multi ? `Coverage · ${covIn.label}` : "Coverage",
      top: y,
      height,
      toggleable: true,
      collapsed,
    };
    covTracks.push(band);
    bands.push(band);
    y += height + 1;
  }
  const cov = covTracks[0] ?? null;

  let seq: TrackBand | null = null;
  if (tracks.seqAvailable) {
    const collapsed = !tracks.seqExpanded;
    const height = collapsed ? COLLAPSED_TRACK_H : SEQ_BAND_H;
    seq = {
      id: "seq",
      label: "Reference",
      top: y,
      height,
      toggleable: true,
      collapsed,
    };
    bands.push(seq);
    y += height + 1;
  }

  const annTracks: AnnTrackLayout[] = [];
  for (const ann of tracks.annTracks) {
    const collapsed = !ann.expanded;
    const lanes = Math.min(MAX_FEATURE_LANES, Math.max(1, ann.laneCount));
    const height = collapsed
      ? COLLAPSED_TRACK_H
      : ANN_BAND_PAD + lanes * FEATURE_LANE_H;
    const band: AnnTrackLayout = {
      id: `ann:${ann.path}`,
      path: ann.path,
      label: ann.label,
      top: y,
      height,
      toggleable: true,
      collapsed,
      laneCount: collapsed ? 0 : lanes,
    };
    annTracks.push(band);
    bands.push(band);
    y += height + 1;
  }

  const readsTracks: ReadsTrackLayout[] = [];
  let readsY = 0;
  for (const rd of tracks.readsTracks) {
    const collapsed = !rd.expanded;
    const densityOnly = !collapsed && rd.densityOnly;
    const laneCount = collapsed || densityOnly ? 0 : Math.max(1, rd.laneCount);
    const height = collapsed
      ? COLLAPSED_TRACK_H
      : densityOnly
        ? READS_HEADER_H + DENSITY_STRIP_H
        : READS_HEADER_H + laneCount * READ_LANE_H + 4;
    const multi = tracks.readsTracks.length > 1;
    const layout: ReadsTrackLayout = {
      id: `reads:${rd.path}`,
      path: rd.path,
      label: multi ? `Alignments · ${rd.label}` : "Alignments",
      top: readsY,
      headerH: collapsed ? COLLAPSED_TRACK_H : READS_HEADER_H,
      laneH: READ_LANE_H,
      laneCount,
      height,
      collapsed,
      densityOnly,
    };
    readsTracks.push(layout);
    bands.push({
      id: layout.id,
      label: layout.label,
      top: y + readsY,
      height,
      toggleable: true,
      collapsed,
    });
    readsY += height + (tracks.readsTracks.length > 1 ? 1 : 0);
  }
  const reads = readsTracks[0] ?? null;
  const readsHeight = Math.max(0, readsY);

  return {
    padL,
    padR,
    usable,
    overview,
    ruler,
    seq,
    cov,
    covTracks,
    annTracks,
    reads,
    readsTracks,
    fixedHeight: y,
    readsHeight,
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
    seqAvailable: tracks.showSeq,
    seqExpanded: tracks.showSeq,
    covTracks: tracks.showCov
      ? [{ path: "_", label: "Coverage", expanded: true }]
      : [],
    annTracks: tracks.showFeat
      ? [{ path: "_", label: "Features", expanded: true, laneCount: featureLaneCount }]
      : [],
    readsTracks: tracks.showReads
      ? [{ path: "_", label: "Alignments", expanded: true, laneCount: 1, densityOnly: false }]
      : [],
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
