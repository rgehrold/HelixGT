import {
  BASE_LETTERS_MAX_BP,
  COVERAGE_FIXED_SCALE,
  COVERAGE_Y_FLOOR,
  DENSITY_STRIP_H,
  READ_BAR_H,
  READ_DETAIL_MIN_PX_PER_BP,
  READ_FETCH_MAX_BP,
  READ_MISMATCH_MAX_BP,
  SEQ_LETTER_FONT,
} from "./constants";
import {
  baseColor,
  baseColorWithQuality,
  featureColor,
  IGV_INDEL_DEL,
  IGV_INDEL_INS,
  IGV_OVERVIEW_THUMB,
  readFillColor,
  themeColors,
} from "./colors";
import { formatGenomicPos, type UnifiedLayout, xForGenomic } from "./geometry";
import type { PackedRead } from "./reads";
import type {
  AlignmentRead,
  AnnotationFeature,
  CigarOp,
  CoverageBin,
  SequenceSlice,
} from "$lib/types";

export type LocalBuffer = {
  contig: string;
  start: number;
  end: number;
  sequence: string;
  reverseComplement: boolean;
};

export type AnnTrackPaint = {
  path: string;
  features: AnnotationFeature[];
  truncated: boolean;
};

export type CovTrackPaint = {
  path: string;
  bins: CoverageBin[];
  maxDepth: number;
};

export type PaintFixedArgs = {
  canvas: HTMLCanvasElement;
  layout: UnifiedLayout;
  cssW: number;
  viewStart: number;
  viewEnd: number;
  visibleBp: number;
  contig: string;
  contigLength: number;
  slice: SequenceSlice | null;
  localBuffer: LocalBuffer | null;
  isFetchingSlice: boolean;
  coverageTracks: CovTrackPaint[];
  overviewBins: CoverageBin[];
  overviewMax: number;
  isFetchingAlign: boolean;
  annTracks: AnnTrackPaint[];
  selectedFeature: AnnotationFeature | null;
  selectedCoverage: CoverageBin | null;
  selectionStart: number | null;
  selectionEnd: number | null;
  colorBases: boolean;
};

export type ReadsTrackPaint = {
  path: string;
  packed: PackedRead[];
  hiddenCount: number;
  truncated: boolean;
  total: number;
  coverageBins: CoverageBin[];
  coverageMax: number;
};

export type PaintReadsArgs = {
  canvas: HTMLCanvasElement;
  layout: UnifiedLayout;
  cssW: number;
  viewStart: number;
  viewEnd: number;
  visibleBp: number;
  tracks: ReadsTrackPaint[];
  localBuffer: LocalBuffer | null;
  contig: string;
  isFetchingAlign: boolean;
  selectedRead: AlignmentRead | null;
  colorReadsBy: "strand" | "mapq" | "pair" | "none";
  colorBases: boolean;
  colorMismatches?: boolean;
  selectionStart: number | null;
  selectionEnd: number | null;
  isPanning?: boolean;
};

const canvasMeta = new WeakMap<
  HTMLCanvasElement,
  { cssW: number; cssH: number; dpr: number; ctx: CanvasRenderingContext2D }
>();

function setupCanvas(canvas: HTMLCanvasElement, cssW: number, cssH: number) {
  const dpr = window.devicePixelRatio || 1;
  const prev = canvasMeta.get(canvas);
  if (
    prev &&
    prev.cssW === cssW &&
    prev.cssH === cssH &&
    prev.dpr === dpr &&
    prev.ctx
  ) {
    return prev.ctx;
  }
  canvas.width = Math.floor(cssW * dpr);
  canvas.height = Math.floor(cssH * dpr);
  canvas.style.width = `${cssW}px`;
  canvas.style.height = `${cssH}px`;
  const ctx = canvas.getContext("2d", { alpha: false });
  if (!ctx) return null;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  canvasMeta.set(canvas, { cssW, cssH, dpr, ctx });
  return ctx;
}

function matchesMatch(op: string): boolean {
  return op === "M" || op === "=" || op === "X";
}

/** Normalize CIGAR op letter (engine always sends single chars; be defensive). */
function cigarOpChar(op: string | undefined): string {
  const s = String(op ?? "").trim().toUpperCase();
  return s.length > 0 ? s[0]! : "";
}

/** Parse SAM CIGAR string when structured ops are missing (avoids all-M fallback). */
function parseCigarString(cigar: string | undefined): CigarOp[] {
  if (!cigar || cigar === "*") return [];
  const ops: CigarOp[] = [];
  let num = 0;
  let sawDigit = false;
  for (const ch of cigar) {
    if (ch >= "0" && ch <= "9") {
      num = num * 10 + (ch.charCodeAt(0) - 48);
      sawDigit = true;
    } else if (/[MIDNSHP=X]/i.test(ch)) {
      ops.push({ op: ch.toUpperCase(), length: sawDigit ? num : 0 });
      num = 0;
      sawDigit = false;
    }
  }
  return ops.filter((o) => o.length > 0);
}

/**
 * Prefer structured cigarOps; fall back to parsing `cigar` text.
 * Never invent a single M-span when the string is available — that mis-aligns
 * inserted bases onto the reference and paints them as base-colored “SNVs”.
 */
function resolveCigarOps(r: AlignmentRead): CigarOp[] {
  if (r.cigarOps?.length) {
    return r.cigarOps.map((o) => ({
      op: cigarOpChar(o.op),
      length: Number(o.length) || 0,
    }));
  }
  const parsed = parseCigarString(r.cigar);
  if (parsed.length) return parsed;
  return [{ op: "M", length: Math.max(1, r.end - r.start) }];
}

function refForMismatch(
  localBuffer: LocalBuffer | null,
  contig: string,
): LocalBuffer | null {
  if (!localBuffer || localBuffer.contig !== contig) return null;
  if (localBuffer.reverseComplement) return null;
  return localBuffer;
}

function drawBandDivider(ctx: CanvasRenderingContext2D, y: number, cssW: number, border: string) {
  ctx.strokeStyle = border;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, y + 0.5);
  ctx.lineTo(cssW, y + 0.5);
  ctx.stroke();
}

function drawSelection(
  ctx: CanvasRenderingContext2D,
  viewStart: number,
  viewEnd: number,
  visibleBp: number,
  padL: number,
  usable: number,
  top: number,
  height: number,
  selectionStart: number | null,
  selectionEnd: number | null,
  color: string,
) {
  if (selectionStart == null || selectionEnd == null || visibleBp <= 0) return;
  const s = Math.max(viewStart, selectionStart);
  const e = Math.min(viewEnd, selectionEnd);
  if (e <= s) return;
  const x0 = xForGenomic(s, viewStart, visibleBp, padL, usable);
  const x1 = xForGenomic(e, viewStart, visibleBp, padL, usable);
  ctx.fillStyle = color;
  ctx.fillRect(x0, top, Math.max(1, x1 - x0), height);
}

export function paintFixedTracks(args: PaintFixedArgs) {
  const {
    canvas,
    layout,
    cssW,
    viewStart,
    viewEnd,
    visibleBp,
    contigLength,
    slice,
    localBuffer,
    isFetchingSlice,
    coverageTracks,
    overviewBins,
    overviewMax,
    isFetchingAlign,
    annTracks,
    selectedFeature,
    selectedCoverage,
    selectionStart,
    selectionEnd,
    contig,
    colorBases,
  } = args;

  const cssH = layout.fixedHeight;
  const ctx = setupCanvas(canvas, cssW, cssH);
  if (!ctx) return;
  const colors = themeColors();
  const { padL, usable } = layout;

  ctx.fillStyle = colors.bg;
  ctx.fillRect(0, 0, cssW, cssH);

  drawSelection(
    ctx,
    viewStart,
    viewEnd,
    visibleBp,
    padL,
    usable,
    0,
    cssH,
    selectionStart,
    selectionEnd,
    colors.selection,
  );

  // ── Overview (ideogram) ──────────────────────────────────────────
  const ov = layout.overview;
  const ovY = ov.top + 2;
  const ovH = ov.height - 4;
  ctx.fillStyle = colors.track;
  ctx.fillRect(padL, ovY, usable, ovH);
  if (contigLength > 0 && overviewBins.length > 0) {
    const scale = Math.max(COVERAGE_Y_FLOOR, overviewMax || 1);
    ctx.fillStyle = colors.coverage;
    for (const bin of overviewBins) {
      const x0 = padL + (bin.start / contigLength) * usable;
      const x1 = padL + (bin.end / contigLength) * usable;
      const h = Math.max(1, Math.min(ovH, (bin.depth / scale) * ovH));
      ctx.fillRect(x0, ovY + ovH - h, Math.max(1, x1 - x0), h);
    }
  }
  if (contigLength > 0) {
    const o0 = (viewStart / contigLength) * usable;
    const o1 = (viewEnd / contigLength) * usable;
    ctx.fillStyle = IGV_OVERVIEW_THUMB;
    ctx.fillRect(padL + o0, ovY, Math.max(2, o1 - o0), ovH);
  }
  drawBandDivider(ctx, ov.top + ov.height, cssW, colors.border);

  // ── Ruler ────────────────────────────────────────────────────────
  const ruler = layout.ruler;
  const rulerLineY = ruler.top + ruler.height - 6;
  ctx.strokeStyle = colors.border;
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(padL, rulerLineY);
  ctx.lineTo(padL + usable, rulerLineY);
  ctx.stroke();

  ctx.fillStyle = colors.muted;
  ctx.font = "11px Arial, Helvetica, sans-serif";
  ctx.textAlign = "center";
  const tickCount = Math.min(12, Math.max(2, Math.floor(usable / 72)));
  for (let i = 0; i <= tickCount; i++) {
    const t = i / tickCount;
    const x = padL + t * usable;
    const pos = Math.floor(viewStart + t * visibleBp);
    ctx.beginPath();
    ctx.moveTo(x, rulerLineY - 3);
    ctx.lineTo(x, rulerLineY + 3);
    ctx.stroke();
    ctx.fillText(formatGenomicPos(pos + 1), x, ruler.top + 11);
  }
  ctx.textAlign = "left";
  drawBandDivider(ctx, ruler.top + ruler.height, cssW, colors.border);

  // ── Coverage (above reference — IGV-like stack) ──────────────────
  for (const band of layout.covTracks) {
    if (band.collapsed) {
      ctx.fillStyle = colors.track;
      ctx.fillRect(padL, band.top + 1, usable, band.height - 2);
      drawBandDivider(ctx, band.top + band.height, cssW, colors.border);
      continue;
    }
    const data = coverageTracks.find((t) => t.path === band.path) ?? coverageTracks[0];
    const bins = data?.bins ?? [];
    const maxDepth = data?.maxDepth ?? 0;
    const barTop = band.top + 4;
    const barH = band.height - 8;
    ctx.fillStyle = colors.track;
    ctx.fillRect(padL, barTop, usable, barH);

    const scale = Math.max(COVERAGE_Y_FLOOR, maxDepth || COVERAGE_FIXED_SCALE);
    if (visibleBp > 0 && bins.length > 0) {
      const grey = colors.coverage;
      const usableH = Math.max(1, barH - 2);
      for (const bin of bins) {
        if (bin.end <= viewStart) continue;
        if (bin.start >= viewEnd) break;
        const g0 = Math.max(bin.start, viewStart);
        const g1 = Math.min(bin.end, viewEnd);
        const x0 = xForGenomic(g0, viewStart, visibleBp, padL, usable);
        const x1 = xForGenomic(g1, viewStart, visibleBp, padL, usable);
        const w = Math.max(1, x1 - x0);
        const h = Math.max(1, Math.min(usableH, (bin.depth / scale) * usableH));
        const selected =
          selectedCoverage?.start === bin.start && selectedCoverage?.end === bin.end;
        ctx.fillStyle = selected ? "#e8d44d" : grey;
        ctx.fillRect(x0, barTop + barH - h, w, h);
      }
    }

    ctx.fillStyle = colors.muted;
    ctx.font = "10px Arial, Helvetica, sans-serif";
    ctx.textAlign = "right";
    ctx.fillText(
      isFetchingAlign ? "…" : `max ${maxDepth.toFixed(0)}×`,
      padL + usable,
      band.top + 11,
    );
    ctx.textAlign = "left";
    drawBandDivider(ctx, band.top + band.height, cssW, colors.border);
  }

  // ── Reference sequence track (between coverage and alignments) ───
  if (layout.seq) {
    const band = layout.seq;
    if (band.collapsed) {
      ctx.fillStyle = colors.track;
      ctx.fillRect(padL, band.top + 1, usable, band.height - 2);
      drawBandDivider(ctx, band.top + band.height, cssW, colors.border);
    } else {
    ctx.fillStyle = colors.track;
    ctx.fillRect(padL, band.top + 1, usable, band.height - 2);

    // Prefer full local buffer for random access; fall back to slice.
    const full =
      localBuffer && localBuffer.contig === contig ? localBuffer : null;
    const hasSeq =
      (full && full.sequence.length > 0) ||
      (slice && slice.sequence.length > 0 && slice.contig === contig);

    if (hasSeq && visibleBp > 0) {
      const letters = visibleBp <= BASE_LETTERS_MAX_BP;
      if (letters) {
        const cell = usable / visibleBp;
        // Semi-bold mono: thicker than Arial, stable width for A/C/G/T columns.
        const px = Math.min(16, Math.max(10, cell * 0.95));
        ctx.font = `600 ${px}px ${SEQ_LETTER_FONT}`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        const yMid = band.top + band.height / 2;
        for (let g = viewStart; g < viewEnd; g++) {
          let base = "";
          if (full && g >= full.start && g < full.end) {
            base = full.sequence[g - full.start] ?? "";
          } else if (slice && g >= slice.start && g < slice.end) {
            base = slice.sequence[g - slice.start] ?? "";
          }
          if (!base) continue;
          const x = padL + (g - viewStart + 0.5) * cell;
          // Always color ref bases so the track is obviously a sequence track.
          ctx.fillStyle = baseColor(base, colors.muted, true);
          ctx.fillText(base.toUpperCase(), x, yMid);
        }
        ctx.textAlign = "left";
        ctx.textBaseline = "alphabetic";
      } else {
        const cols = Math.ceil(usable);
        for (let col = 0; col < cols; col++) {
          const g = Math.floor(viewStart + (col / usable) * visibleBp);
          if (g >= viewEnd) break;
          let base = "";
          if (full && g >= full.start && g < full.end) {
            base = full.sequence[g - full.start] ?? "";
          } else if (slice && g >= slice.start && g < slice.end) {
            base = slice.sequence[g - slice.start] ?? "";
          }
          if (!base) continue;
          ctx.fillStyle = baseColor(base, colors.track, true);
          ctx.fillRect(padL + col, band.top + 3, 1, band.height - 6);
        }
      }
    } else {
      ctx.fillStyle = colors.muted;
      ctx.font = "11px Arial, Helvetica, sans-serif";
      ctx.fillText(
        isFetchingSlice
          ? "Loading reference…"
          : "Open a reference FASTA (or Set as reference) to show this track",
        padL + 4,
        band.top + band.height / 2 + 4,
      );
    }
    drawBandDivider(ctx, band.top + band.height, cssW, colors.border);
    }
  }

  // ── Annotation tracks (one per file) ─────────────────────────────
  for (const annBand of layout.annTracks) {
    if (annBand.collapsed) {
      ctx.fillStyle = colors.track;
      ctx.fillRect(padL, annBand.top + 1, usable, annBand.height - 2);
      drawBandDivider(ctx, annBand.top + annBand.height, cssW, colors.border);
      continue;
    }
    const data = annTracks.find((t) => t.path === annBand.path);
    const feats = data?.features ?? [];
    const truncated = data?.truncated ?? false;

    ctx.fillStyle = colors.track;
    ctx.fillRect(padL, annBand.top + 2, usable, annBand.height - 4);

    if (visibleBp > 0 && feats.length > 0) {
      const ends: number[] = [];
      for (const f of feats) {
        if (f.end <= viewStart) continue;
        if (f.start >= viewEnd) break;
        const x0 = xForGenomic(
          Math.max(f.start, viewStart),
          viewStart,
          visibleBp,
          padL,
          usable,
        );
        const x1 = xForGenomic(
          Math.min(f.end, viewEnd),
          viewStart,
          visibleBp,
          padL,
          usable,
        );
        const w = Math.max(1, x1 - x0);
        let lane = 0;
        while (lane < ends.length && ends[lane]! > f.start) lane++;
        if (lane === ends.length) ends.push(0);
        ends[lane] = f.end;
        if (lane >= annBand.laneCount) continue;
        const y = annBand.top + 4 + lane * 11;
        const h = 8;
        const selected =
          selectedFeature?.id === f.id &&
          selectedFeature.name === f.name &&
          selectedFeature.source === f.source;
        ctx.fillStyle = featureColor(f, colors);
        ctx.fillRect(x0, y, w, h);
        if (w > 10) {
          ctx.beginPath();
          if (f.strand === "-") {
            ctx.moveTo(x0, y);
            ctx.lineTo(x0 - 3, y + h / 2);
            ctx.lineTo(x0, y + h);
          } else if (f.strand === "+") {
            ctx.moveTo(x0 + w, y);
            ctx.lineTo(x0 + w + 3, y + h / 2);
            ctx.lineTo(x0 + w, y + h);
          }
          ctx.fill();
        }
        if ((selected || w > 40) && w > 24) {
          ctx.fillStyle = colors.text;
          ctx.font = "10px Arial, Helvetica, sans-serif";
          ctx.fillText(f.name || f.featureType, x0 + 2, y + h - 1, Math.max(8, w - 4));
        }
      }
    }

    if (truncated) {
      ctx.fillStyle = colors.muted;
      ctx.font = "10px Arial, Helvetica, sans-serif";
      ctx.fillText("truncated", padL + usable - 52, annBand.top + 11);
    }
    drawBandDivider(ctx, annBand.top + annBand.height, cssW, colors.border);
  }
}

export function paintReadsTrack(args: PaintReadsArgs) {
  const {
    canvas,
    layout,
    cssW,
    viewStart,
    viewEnd,
    visibleBp,
    tracks,
    localBuffer,
    contig,
    isFetchingAlign,
    selectedRead,
    colorReadsBy,
    colorBases,
    colorMismatches = true,
    selectionStart,
    selectionEnd,
    isPanning = false,
  } = args;

  if (layout.readsTracks.length === 0) return;
  const height = Math.max(1, layout.readsHeight || layout.reads?.height || 1);
  const ctx = setupCanvas(canvas, cssW, height);
  if (!ctx) return;
  const colors = themeColors();
  const { padL, usable } = layout;

  ctx.fillStyle = colors.bg;
  ctx.fillRect(0, 0, cssW, height);

  const ref = refForMismatch(localBuffer, contig);
  const pxPerBp = visibleBp > 0 ? usable / visibleBp : 0;

  for (const band of layout.readsTracks) {
    const data = tracks.find((t) => t.path === band.path);
    const packed = data?.packed ?? [];
    const hiddenCount = data?.hiddenCount ?? 0;
    const readsTruncated = data?.truncated ?? false;
    const readsTotal = data?.total ?? 0;

    if (band.collapsed) {
      ctx.fillStyle = colors.track;
      ctx.fillRect(padL, band.top + 1, usable, band.height - 2);
      ctx.fillStyle = colors.muted;
      ctx.font = "10px Arial, Helvetica, sans-serif";
      ctx.fillText(band.label, padL + 4, band.top + 12);
      continue;
    }

    const { headerH } = band;
    ctx.fillStyle = colors.muted;
    ctx.font = "10px Arial, Helvetica, sans-serif";
    const statusParts: string[] = [];
    if (layout.readsTracks.length > 1) statusParts.push(band.label);
    if (isFetchingAlign) statusParts.push("loading…");
    else if (band.densityOnly) {
      statusParts.push(`Zoom in (≤${Math.round(READ_FETCH_MAX_BP / 1000)} kb) for pileup`);
    } else {
      statusParts.push(`${band.laneCount} lane${band.laneCount === 1 ? "" : "s"}`);
      statusParts.push(`${packed.length} read${packed.length === 1 ? "" : "s"}`);
      if (hiddenCount > 0) statusParts.push(`+${hiddenCount} hidden`);
      if (readsTruncated) statusParts.push("sampled");
      if (readsTotal > packed.length + hiddenCount) {
        statusParts.push(`${readsTotal.toLocaleString()} in region`);
      }
    }
    ctx.fillText(statusParts.join(" · "), padL, band.top + 12);

    ctx.fillStyle = colors.track;
    ctx.fillRect(padL, band.top + headerH, usable, band.height - headerH);

    drawSelection(
      ctx,
      viewStart,
      viewEnd,
      visibleBp,
      padL,
      usable,
      band.top + headerH,
      band.height - headerH,
      selectionStart,
      selectionEnd,
      colors.selection,
    );

    if (band.densityOnly) {
      paintDensityStrip(
        ctx,
        data?.coverageBins ?? [],
        data?.coverageMax ?? 0,
        viewStart,
        viewEnd,
        visibleBp,
        padL,
        usable,
        band.top + headerH + 2,
        DENSITY_STRIP_H - 4,
        colors.coverage,
      );
      continue;
    }

    const hasSeq = packed.length > 0 && !!(packed[0]?.read.sequence);
    const sample = packed[0]?.read;
    const hasCigar =
      !!sample &&
      ((sample.cigarOps?.length ?? 0) > 0 ||
        (!!sample.cigar && sample.cigar !== "*") ||
        hasSeq);
    const shadeMismatches =
      colorMismatches &&
      !isPanning &&
      !!ref &&
      hasSeq &&
      visibleBp <= READ_MISMATCH_MAX_BP &&
      pxPerBp >= READ_DETAIL_MIN_PX_PER_BP;
    const showMismatchLetters =
      shadeMismatches && colorBases && visibleBp <= BASE_LETTERS_MAX_BP;
    const shadeIndels =
      colorMismatches &&
      !isPanning &&
      hasCigar &&
      visibleBp <= READ_MISMATCH_MAX_BP &&
      pxPerBp >= READ_DETAIL_MIN_PX_PER_BP * 0.6;
    const detailCigar =
      !isPanning &&
      (shadeMismatches || shadeIndels || pxPerBp >= READ_DETAIL_MIN_PX_PER_BP * 1.5);

    if (!isPanning) {
      drawPairConnectors(
        ctx,
        packed,
        viewStart,
        visibleBp,
        padL,
        usable,
        band.top + headerH,
        band.laneH,
        contig,
        colors.muted,
      );
    }

    for (const { read: r, lane } of packed) {
      const y = band.top + headerH + 2 + lane * band.laneH;
      const selected =
        selectedRead?.name === r.name &&
        selectedRead.start === r.start &&
        selectedRead.flags === r.flags;

      if (!detailCigar) {
        drawReadBarSimple(
          ctx,
          r,
          padL,
          usable,
          y,
          READ_BAR_H,
          selected,
          colors,
          colorReadsBy,
          viewStart,
          visibleBp,
        );
      } else {
        drawReadBar(
          ctx,
          r,
          padL,
          usable,
          y,
          READ_BAR_H,
          selected,
          showMismatchLetters,
          shadeMismatches,
          shadeIndels,
          ref,
          colors,
          colorReadsBy,
          viewStart,
          visibleBp,
        );
      }
    }

    if (packed.length === 0) {
      ctx.fillStyle = colors.muted;
      ctx.font = "11px Arial, Helvetica, sans-serif";
      ctx.fillText(
        visibleBp > READ_FETCH_MAX_BP
          ? `Zoom in (≤${Math.round(READ_FETCH_MAX_BP / 1000)} kb) for read pileup — coverage only at this zoom`
          : isFetchingAlign
            ? "Loading alignments…"
            : "No alignments in window",
        padL + 6,
        band.top + headerH + 20,
      );
    }
  }
}

function paintDensityStrip(
  ctx: CanvasRenderingContext2D,
  bins: CoverageBin[],
  maxDepth: number,
  viewStart: number,
  viewEnd: number,
  visibleBp: number,
  padL: number,
  usable: number,
  top: number,
  height: number,
  color: string,
) {
  if (visibleBp <= 0 || bins.length === 0) return;
  const scale = Math.max(COVERAGE_Y_FLOOR, maxDepth || COVERAGE_FIXED_SCALE);
  ctx.fillStyle = color;
  for (const bin of bins) {
    if (bin.end <= viewStart) continue;
    if (bin.start >= viewEnd) break;
    const x0 = xForGenomic(Math.max(bin.start, viewStart), viewStart, visibleBp, padL, usable);
    const x1 = xForGenomic(Math.min(bin.end, viewEnd), viewStart, visibleBp, padL, usable);
    const h = Math.max(1, Math.min(height, (bin.depth / scale) * height));
    ctx.fillRect(x0, top + height - h, Math.max(1, x1 - x0), h);
  }
}

function drawPairConnectors(
  ctx: CanvasRenderingContext2D,
  packed: PackedRead[],
  viewStart: number,
  visibleBp: number,
  padL: number,
  usable: number,
  headerTop: number,
  laneH: number,
  contig: string,
  color: string,
) {
  if (visibleBp <= 0) return;
  const viewEnd = viewStart + visibleBp;
  ctx.strokeStyle = color;
  ctx.globalAlpha = 0.35;
  ctx.lineWidth = 1;
  for (const { read: r, lane } of packed) {
    if (!r.isPaired || r.mateStart == null) continue;
    if (r.mateContig && r.mateContig !== "=" && r.mateContig !== contig) continue;
    const mate = r.mateStart;
    if (mate <= r.start) continue;
    if (r.end >= viewEnd && mate >= viewEnd) continue;
    if (r.start < viewStart && mate < viewStart) continue;
    const x0 = xForGenomic(Math.min(r.end, viewEnd), viewStart, visibleBp, padL, usable);
    const x1 = xForGenomic(Math.max(mate, viewStart), viewStart, visibleBp, padL, usable);
    if (x1 <= x0) continue;
    const y = headerTop + 2 + lane * laneH + READ_BAR_H / 2;
    ctx.beginPath();
    ctx.moveTo(x0, y);
    ctx.lineTo(x1, y);
    ctx.stroke();
  }
  ctx.globalAlpha = 1;
}

/** Fast path: single fillRect for the read span (no CIGAR ops). */
function drawReadBarSimple(
  ctx: CanvasRenderingContext2D,
  r: AlignmentRead,
  padL: number,
  usable: number,
  y: number,
  h: number,
  selected: boolean,
  colors: ReturnType<typeof themeColors>,
  colorBy: "strand" | "mapq" | "pair" | "none",
  viewStart: number,
  visibleBp: number,
) {
  if (visibleBp <= 0) return;
  const viewEnd = viewStart + visibleBp;
  if (r.end <= viewStart || r.start >= viewEnd) return;
  const x0 = xForGenomic(Math.max(r.start, viewStart), viewStart, visibleBp, padL, usable);
  const x1 = xForGenomic(Math.min(r.end, viewEnd), viewStart, visibleBp, padL, usable);
  const w = Math.max(1, x1 - x0);
  ctx.fillStyle = readFillColor(r, selected, colorBy, colors);
  ctx.globalAlpha = r.mapq < 10 ? 0.55 : 1;
  ctx.fillRect(x0, y, w, h);
  ctx.globalAlpha = 1;
  if (selected) {
    ctx.strokeStyle = "#333333";
    ctx.lineWidth = 1;
    ctx.strokeRect(x0 + 0.5, y + 0.5, w - 1, h - 1);
  }
}

function phredAt(qualities: string | undefined, index: number): number {
  if (!qualities || index < 0 || index >= qualities.length) return 20;
  return Math.max(0, qualities.charCodeAt(index) - 33);
}

function drawReadBar(
  ctx: CanvasRenderingContext2D,
  r: AlignmentRead,
  padL: number,
  usable: number,
  y: number,
  h: number,
  selected: boolean,
  /** When true, draw the alternate base letter on mismatches only (never on matches). */
  showMismatchLetters: boolean,
  shadeMismatches: boolean,
  /** When true, paint CIGAR insertions (I) and deletions (D) / skips (N). */
  shadeIndels: boolean,
  refBuf: LocalBuffer | null,
  colors: ReturnType<typeof themeColors>,
  colorBy: "strand" | "mapq" | "pair" | "none",
  viewStart: number,
  visibleBp: number,
) {
  if (visibleBp <= 0) return;
  const viewEnd = viewStart + visibleBp;

  // Body first (always). Soft-clips / mismatches / indels optional and cheaper.
  drawReadBarSimple(
    ctx,
    r,
    padL,
    usable,
    y,
    h,
    selected,
    colors,
    colorBy,
    viewStart,
    visibleBp,
  );

  const cigarOps = resolveCigarOps(r);
  const hasRealCigar =
    (r.cigarOps?.length ?? 0) > 0 || (!!r.cigar && r.cigar !== "*");
  const wantMismatch = shadeMismatches && !!r.sequence && !!refBuf;
  // Indels whenever we have real CIGAR (ops or string), not only structured ops.
  const wantIndel = shadeIndels && hasRealCigar;
  if (!wantMismatch && !wantIndel) return;

  let refPos = r.start;
  let seqPos = 0;
  const cell = usable / visibleBp;
  // Skip dense per-base SNV work when sub-pixel; indels still cheap.
  const paintSnvs = wantMismatch && (cell >= 0.75 || showMismatchLetters);
  // Hard purple so inserts never pick up base/theme confusion.
  const insColor = colors.indelIns || IGV_INDEL_INS;
  const delColor = colors.indelDel || IGV_INDEL_DEL;

  if (showMismatchLetters) {
    const px = Math.min(h - 2, Math.max(9, Math.min(cell * 0.9, h - 2)));
    ctx.font = `700 ${px}px ${SEQ_LETTER_FONT}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
  }
  const letterY = y + h / 2 + 1;
  /** Draw inserts last so purple carets sit on top of any SNV ticks. */
  const insertions: { refPos: number; len: number }[] = [];

  for (const op of cigarOps) {
    const len = Number(op.length) || 0;
    const opChar = cigarOpChar(op.op);
    if (len <= 0) continue;
    if (opChar === "S" || opChar === "H") {
      if (opChar === "S" && wantIndel) {
        const clipStart = refPos === r.start && seqPos === 0 ? r.start - len : refPos;
        const clipEnd = clipStart + len;
        if (clipEnd > viewStart && clipStart < viewEnd) {
          const x0 = xForGenomic(Math.max(clipStart, viewStart), viewStart, visibleBp, padL, usable);
          const x1 = xForGenomic(Math.min(clipEnd, viewEnd), viewStart, visibleBp, padL, usable);
          ctx.globalAlpha = 0.4;
          ctx.fillStyle = colors.readGray;
          ctx.fillRect(x0, y + 2, Math.max(1, x1 - x0), Math.max(2, h - 4));
          ctx.globalAlpha = 1;
        }
      }
      if (opChar === "S") seqPos += len;
      continue;
    }
    if (opChar === "P") continue;

    // Insertion: zero-width on reference — do NOT walk sequence against ref
    // (that paints inserts as base-colored “mismatches”).
    if (opChar === "I") {
      if (wantIndel && refPos >= viewStart && refPos <= viewEnd) {
        insertions.push({ refPos, len });
      }
      seqPos += len;
      continue;
    }

    // Deletion (D): black bar over deleted reference bases.
    // Skip/intron (N): thinner midline (RNA-style).
    if (opChar === "D" || opChar === "N") {
      if (wantIndel) {
        const s = refPos;
        const e = refPos + len;
        if (e > viewStart && s < viewEnd) {
          const x0 = xForGenomic(Math.max(s, viewStart), viewStart, visibleBp, padL, usable);
          const x1 = xForGenomic(Math.min(e, viewEnd), viewStart, visibleBp, padL, usable);
          const w = Math.max(1, x1 - x0);
          ctx.fillStyle = delColor;
          if (opChar === "D") {
            ctx.fillRect(x0, y, w, h);
            if (showMismatchLetters && len >= 1 && w >= 8) {
              ctx.fillStyle = colors.bg;
              ctx.font = `700 ${Math.min(h - 2, 11)}px ${SEQ_LETTER_FONT}`;
              ctx.textAlign = "center";
              ctx.textBaseline = "middle";
              ctx.fillText(len > 1 ? String(len) : "–", x0 + w / 2, letterY);
              const px = Math.min(h - 2, Math.max(9, Math.min(cell * 0.9, h - 2)));
              ctx.font = `700 ${px}px ${SEQ_LETTER_FONT}`;
            }
          } else {
            const mid = y + h / 2 - 1;
            ctx.fillRect(x0, mid, w, 2);
          }
        }
      }
      refPos += len;
      continue;
    }

    if (!matchesMatch(opChar)) continue;

    const s = refPos;
    const e = refPos + len;
    if (e <= viewStart) {
      refPos += len;
      seqPos += len;
      continue;
    }
    if (s >= viewEnd) break;

    // X is sequence mismatch vs ref encoding — still walk like M for base paint.
    if (paintSnvs && refBuf && r.sequence) {
      const from = Math.max(s, viewStart);
      const to = Math.min(e, viewEnd);
      for (let p = from; p < to; p++) {
        const iSeq = seqPos + (p - s);
        const base = r.sequence[iSeq] ?? "";
        if (!base) continue;
        if (p < refBuf.start || p >= refBuf.end) continue;
        const rb = refBuf.sequence[p - refBuf.start] ?? "";
        if (!rb || rb.toUpperCase() === base.toUpperCase()) continue;
        const q = phredAt(r.qualities, iSeq);
        const x = padL + (p - viewStart + 0.5) * cell;
        const fill = baseColorWithQuality(base, q, colors.muted);
        ctx.fillStyle = fill;
        if (showMismatchLetters) {
          ctx.fillText(base.toUpperCase(), x, letterY);
        } else {
          ctx.fillRect(x - cell / 2, y, Math.max(1, cell), h);
        }
      }
    }
    refPos += len;
    seqPos += len;
  }

  // Purple insertion carets on top (never base-colored).
  for (const ins of insertions) {
    drawInsertionMark(
      ctx,
      ins.refPos,
      viewStart,
      visibleBp,
      padL,
      usable,
      y,
      h,
      ins.len,
      insColor,
    );
  }

  if (showMismatchLetters) {
    ctx.textAlign = "left";
    ctx.textBaseline = "alphabetic";
  }
}

/**
 * IGV-style insertion: purple I-beam / caret at the insert locus
 * (vertical stroke + short end caps). No “+” glyphs — length only
 * widens the mark slightly for multi-base inserts.
 */
function drawInsertionMark(
  ctx: CanvasRenderingContext2D,
  refPos: number,
  viewStart: number,
  visibleBp: number,
  padL: number,
  usable: number,
  y: number,
  h: number,
  insertLen: number,
  insColor: string,
) {
  const cell = usable / visibleBp;
  const x = padL + (refPos - viewStart) * cell;
  // Slightly thicker for multi-base inserts; still a pure purple mark.
  const stroke = Math.max(
    2,
    Math.min(insertLen > 1 ? 6 : 5, cell * (insertLen > 1 ? 0.55 : 0.4)),
  );
  ctx.fillStyle = insColor;
  // Vertical through the read bar.
  ctx.fillRect(x - stroke / 2, y, stroke, h);
  // End caps (I-beam) — classic IGV insertion glyph.
  const tickW = Math.max(stroke + 2, Math.min(12, cell * 0.95));
  ctx.fillRect(x - tickW / 2, y, tickW, 2);
  ctx.fillRect(x - tickW / 2, y + h - 2, tickW, 2);
}
