import {
  BASE_LETTERS_MAX_BP,
  MISMATCH_PROFILE_MAX_BP,
  READ_BAR_H,
  READ_DETAIL_MIN_PX_PER_BP,
  READ_FETCH_MAX_BP,
} from "./constants";
import {
  baseColor,
  coverageBarColor,
  featureColor,
  IGV_OVERVIEW_THUMB,
  readFillColor,
  themeColors,
} from "./colors";
import { formatGenomicPos, type UnifiedLayout, xForGenomic } from "./geometry";
import { mismatchFractions, type PackedRead } from "./reads";
import type {
  AlignmentRead,
  AnnotationFeature,
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
  coverageBins: CoverageBin[];
  coverageMax: number;
  isFetchingAlign: boolean;
  annTracks: AnnTrackPaint[];
  alignmentReads: AlignmentRead[];
  selectedFeature: AnnotationFeature | null;
  selectedCoverage: CoverageBin | null;
  selectionStart: number | null;
  selectionEnd: number | null;
  colorMismatches: boolean;
  colorBases: boolean;
};

export type PaintReadsArgs = {
  canvas: HTMLCanvasElement;
  layout: UnifiedLayout;
  cssW: number;
  viewStart: number;
  viewEnd: number;
  visibleBp: number;
  packed: PackedRead[];
  hiddenCount: number;
  localBuffer: LocalBuffer | null;
  contig: string;
  isFetchingAlign: boolean;
  readsTruncated: boolean;
  readsTotal: number;
  selectedRead: AlignmentRead | null;
  colorReadsBy: "strand" | "mapq" | "pair" | "none";
  colorBases: boolean;
  selectionStart: number | null;
  selectionEnd: number | null;
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
    coverageBins,
    coverageMax,
    isFetchingAlign,
    annTracks,
    selectedFeature,
    selectedCoverage,
    selectionStart,
    selectionEnd,
    alignmentReads,
    contig,
    colorMismatches,
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

  // ── Reference ────────────────────────────────────────────────────
  if (layout.seq) {
    const band = layout.seq;
    ctx.fillStyle = colors.track;
    ctx.fillRect(padL, band.top + 1, usable, band.height - 2);

    const seqData =
      slice ??
      (localBuffer && localBuffer.contig === contig
        ? {
            contig: localBuffer.contig,
            start: Math.max(localBuffer.start, viewStart),
            end: Math.min(localBuffer.end, viewEnd),
            sequence: localBuffer.sequence.slice(
              Math.max(0, viewStart - localBuffer.start),
              Math.max(0, viewEnd - localBuffer.start),
            ),
            contigLength,
            reverseComplemented: localBuffer.reverseComplement,
          }
        : null);

    if (seqData && seqData.sequence.length > 0 && visibleBp > 0) {
      if (visibleBp <= BASE_LETTERS_MAX_BP) {
        const cell = usable / visibleBp;
        ctx.font = `${Math.min(14, Math.max(9, cell * 0.85))}px Arial, Helvetica, sans-serif`;
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        const yMid = band.top + band.height / 2;
        const full =
          localBuffer && localBuffer.contig === contig ? localBuffer : null;
        for (let g = viewStart; g < viewEnd; g++) {
          let base = "";
          if (full && g >= full.start && g < full.end) {
            base = full.sequence[g - full.start] ?? "";
          } else if (g >= seqData.start && g < seqData.end) {
            base = seqData.sequence[g - seqData.start] ?? "";
          }
          if (!base) continue;
          const x = padL + (g - viewStart + 0.5) * cell;
          ctx.fillStyle = baseColor(base, colors.muted, colorBases);
          ctx.fillText(base.toUpperCase(), x, yMid);
        }
        ctx.textAlign = "left";
        ctx.textBaseline = "alphabetic";
      } else {
        // One sample per pixel column — much cheaper than stepping by bp.
        const full =
          localBuffer && localBuffer.contig === contig ? localBuffer : null;
        const cols = Math.ceil(usable);
        for (let col = 0; col < cols; col++) {
          const g = Math.floor(viewStart + (col / usable) * visibleBp);
          if (g >= viewEnd) break;
          let base = "";
          if (full && g >= full.start && g < full.end) {
            base = full.sequence[g - full.start] ?? "";
          } else if (g >= seqData.start && g < seqData.end) {
            base = seqData.sequence[g - seqData.start] ?? "";
          }
          if (!base) continue;
          ctx.fillStyle = baseColor(base, colors.track, colorBases);
          ctx.fillRect(padL + col, band.top + 3, 1, band.height - 6);
        }
      }
    } else {
      ctx.fillStyle = colors.muted;
      ctx.font = "11px Arial, Helvetica, sans-serif";
      ctx.fillText(
        isFetchingSlice ? "Loading reference…" : "Zoom in to load sequence",
        padL + 4,
        band.top + band.height / 2 + 4,
      );
    }
    drawBandDivider(ctx, band.top + band.height, cssW, colors.border);
  }

  // ── Coverage ─────────────────────────────────────────────────────
  if (layout.cov) {
    const band = layout.cov;
    const barTop = band.top + 4;
    const barH = band.height - 8;
    ctx.fillStyle = colors.track;
    ctx.fillRect(padL, barTop, usable, barH);

    const ref = colorMismatches ? refForMismatch(localBuffer, contig) : null;
    const mm =
      colorMismatches && ref
        ? mismatchFractions(
            alignmentReads,
            viewStart,
            viewEnd,
            ref,
            MISMATCH_PROFILE_MAX_BP,
          )
        : null;

    if (visibleBp > 0 && coverageBins.length > 0) {
      const maxD = Math.max(coverageMax, 1);
      // Skip bins fully outside viewport with a linear scan (bins are ordered).
      for (const bin of coverageBins) {
        if (bin.end <= viewStart) continue;
        if (bin.start >= viewEnd) break;
        const x0 = xForGenomic(
          Math.max(bin.start, viewStart),
          viewStart,
          visibleBp,
          padL,
          usable,
        );
        const x1 = xForGenomic(
          Math.min(bin.end, viewEnd),
          viewStart,
          visibleBp,
          padL,
          usable,
        );
        const w = Math.max(1, x1 - x0);
        const h = Math.max(1, (bin.depth / maxD) * (barH - 2));
        const selected =
          selectedCoverage?.start === bin.start && selectedCoverage?.end === bin.end;
        let mmFrac = 0;
        if (mm) {
          let sum = 0;
          let n = 0;
          const a = Math.max(0, Math.floor(bin.start - viewStart));
          const b = Math.min(mm.length, Math.ceil(bin.end - viewStart));
          for (let i = a; i < b; i++) {
            sum += mm[i] ?? 0;
            n++;
          }
          mmFrac = n > 0 ? sum / n : 0;
        }
        ctx.fillStyle = coverageBarColor(mmFrac, colors, colorMismatches, selected);
        ctx.fillRect(x0, barTop + barH - h, w, h);
      }
    }

    ctx.fillStyle = colors.muted;
    ctx.font = "10px Arial, Helvetica, sans-serif";
    ctx.textAlign = "right";
    ctx.fillText(
      isFetchingAlign ? "…" : coverageMax ? `max ${coverageMax.toFixed(0)}×` : "",
      padL + usable,
      band.top + 11,
    );
    ctx.textAlign = "left";
    drawBandDivider(ctx, band.top + band.height, cssW, colors.border);
  }

  // ── Annotation tracks (one per file) ─────────────────────────────
  for (const annBand of layout.annTracks) {
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
    packed,
    hiddenCount,
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
  } = args;

  if (!layout.reads) return;
  const { headerH, laneH, height } = layout.reads;
  const ctx = setupCanvas(canvas, cssW, height);
  if (!ctx) return;
  const colors = themeColors();
  const { padL, usable } = layout;

  ctx.fillStyle = colors.bg;
  ctx.fillRect(0, 0, cssW, height);

  ctx.fillStyle = colors.muted;
  ctx.font = "10px Arial, Helvetica, sans-serif";
  const statusParts: string[] = [];
  if (isFetchingAlign) statusParts.push("loading…");
  else {
    statusParts.push(`${packed.length} lane${packed.length === 1 ? "" : "s"}`);
    if (hiddenCount > 0) statusParts.push(`+${hiddenCount} hidden`);
    if (readsTruncated) statusParts.push("fetch truncated");
    if (readsTotal > packed.length + hiddenCount) {
      statusParts.push(`${readsTotal.toLocaleString()} in region`);
    }
  }
  ctx.fillText(statusParts.join(" · "), padL, 12);

  ctx.fillStyle = colors.track;
  ctx.fillRect(padL, headerH, usable, height - headerH);

  drawSelection(
    ctx,
    viewStart,
    viewEnd,
    visibleBp,
    padL,
    usable,
    headerH,
    height - headerH,
    selectionStart,
    selectionEnd,
    colors.selection,
  );

  const ref = refForMismatch(localBuffer, contig);
  const pxPerBp = visibleBp > 0 ? usable / visibleBp : 0;
  const drawBases = colorBases && visibleBp <= BASE_LETTERS_MAX_BP;
  // When zoomed out, skip CIGAR traversal — solid bars are enough and much faster.
  const detailCigar = pxPerBp >= READ_DETAIL_MIN_PX_PER_BP;

  for (const { read: r, lane } of packed) {
    const y = headerH + 2 + lane * laneH;
    const selected =
      selectedRead?.name === r.name &&
      selectedRead.start === r.start &&
      selectedRead.flags === r.flags;

    if (!detailCigar || (!drawBases && !r.cigarOps?.length)) {
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
        drawBases,
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
        ? "Zoom in to load alignments"
        : isFetchingAlign
          ? "Loading alignments…"
          : "No alignments in window",
      padL + 6,
      headerH + 20,
    );
  }
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

function drawReadBar(
  ctx: CanvasRenderingContext2D,
  r: AlignmentRead,
  padL: number,
  usable: number,
  y: number,
  h: number,
  selected: boolean,
  drawBases: boolean,
  refBuf: LocalBuffer | null,
  colors: ReturnType<typeof themeColors>,
  colorBy: "strand" | "mapq" | "pair" | "none",
  viewStart: number,
  visibleBp: number,
) {
  if (visibleBp <= 0) return;
  const viewEnd = viewStart + visibleBp;
  const ops =
    r.cigarOps?.length > 0
      ? r.cigarOps
      : [{ op: "M", length: Math.max(1, r.end - r.start) }];

  let firstMatch = true;
  let softLead = 0;
  for (const op of ops) {
    if (op.op === "S" && firstMatch) softLead += op.length;
    else if (matchesMatch(op.op) || op.op === "D" || op.op === "N" || op.op === "I") {
      firstMatch = false;
    }
  }
  if (softLead > 0) {
    const clipEnd = r.start;
    const clipStart = Math.max(0, clipEnd - softLead);
    if (clipEnd > viewStart && clipStart < viewEnd) {
      const x0 = xForGenomic(Math.max(clipStart, viewStart), viewStart, visibleBp, padL, usable);
      const x1 = xForGenomic(Math.min(clipEnd, viewEnd), viewStart, visibleBp, padL, usable);
      ctx.fillStyle = "#aaaaaa";
      ctx.fillRect(x0, y + 1, Math.max(1, x1 - x0), h - 2);
    }
  }

  let refPos = r.start;
  let seqPos = 0;
  for (const op of ops) {
    const len = op.length;
    const opChar = op.op;
    if (opChar === "S" || opChar === "H") {
      if (opChar === "S") seqPos += len;
      continue;
    }
    if (opChar === "I" || opChar === "P") {
      if (refPos >= viewStart && refPos < viewEnd) {
        const x = xForGenomic(refPos, viewStart, visibleBp, padL, usable);
        ctx.fillStyle = "#c9a227";
        ctx.fillRect(x - 1, y, 2, h);
      }
      if (opChar === "I") seqPos += len;
      continue;
    }
    if (opChar === "D" || opChar === "N") {
      const s = refPos;
      const e = refPos + len;
      if (e > viewStart && s < viewEnd) {
        const x0 = xForGenomic(Math.max(s, viewStart), viewStart, visibleBp, padL, usable);
        const x1 = xForGenomic(Math.min(e, viewEnd), viewStart, visibleBp, padL, usable);
        ctx.strokeStyle = opChar === "N" ? "#888888" : "#c84a4a";
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(x0, y + h / 2);
        ctx.lineTo(Math.max(x0 + 1, x1), y + h / 2);
        ctx.stroke();
      }
      refPos += len;
      continue;
    }
    if (matchesMatch(opChar)) {
      const s = refPos;
      const e = refPos + len;
      if (e > viewStart && s < viewEnd) {
        const x0 = xForGenomic(Math.max(s, viewStart), viewStart, visibleBp, padL, usable);
        const x1 = xForGenomic(Math.min(e, viewEnd), viewStart, visibleBp, padL, usable);
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

        if (drawBases && r.sequence && visibleBp > 0) {
          const cell = usable / visibleBp;
          ctx.font = `${Math.min(10, Math.max(7, cell * 0.8))}px Arial, Helvetica, sans-serif`;
          ctx.textAlign = "center";
          ctx.textBaseline = "middle";
          const from = Math.max(s, viewStart);
          const to = Math.min(e, viewEnd);
          for (let p = from; p < to; p++) {
            const iSeq = seqPos + (p - s);
            const base = r.sequence[iSeq] ?? "";
            if (!base) continue;
            let mismatch = false;
            if (refBuf && p >= refBuf.start && p < refBuf.end) {
              const rb = refBuf.sequence[p - refBuf.start] ?? "";
              if (rb && rb.toUpperCase() !== base.toUpperCase()) mismatch = true;
            }
            const x = padL + (p - viewStart + 0.5) * cell;
            if (mismatch) {
              ctx.fillStyle = "#c84a4a";
              ctx.fillRect(x - cell / 2, y, Math.max(1, cell), h);
              ctx.fillStyle = "#ffffff";
            } else {
              ctx.fillStyle = "#ffffff";
            }
            ctx.fillText(base.toUpperCase(), x, y + h / 2);
          }
          ctx.textAlign = "left";
          ctx.textBaseline = "alphabetic";
        }
      }
      refPos += len;
      seqPos += len;
    }
  }

  if (ops.length > 0) {
    let trailing = 0;
    for (let i = ops.length - 1; i >= 0; i--) {
      if (ops[i]!.op === "S") trailing += ops[i]!.length;
      else if (ops[i]!.op === "H") continue;
      else break;
    }
    if (trailing > 0) {
      const clipStart = r.end;
      const clipEnd = r.end + trailing;
      if (clipEnd > viewStart && clipStart < viewEnd) {
        const x0 = xForGenomic(Math.max(clipStart, viewStart), viewStart, visibleBp, padL, usable);
        const x1 = xForGenomic(Math.min(clipEnd, viewEnd), viewStart, visibleBp, padL, usable);
        ctx.fillStyle = "#aaaaaa";
        ctx.fillRect(x0, y + 1, Math.max(1, x1 - x0), h - 2);
      }
    }
  }
}
