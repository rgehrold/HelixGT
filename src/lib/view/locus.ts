/** Parse and format IGV-style locus strings (1-based coordinates in UI). */

export function formatLocus(contig: string, viewStart: number, viewEnd: number): string {
  const a = viewStart + 1;
  const b = viewEnd;
  if (a >= b) return `${contig}:${formatPos(a)}`;
  return `${contig}:${formatPos(a)}-${formatPos(b)}`;
}

export function formatPos(pos1Based: number): string {
  return pos1Based.toLocaleString("en-US");
}

export type LocusParseResult =
  | { ok: true; contig: string; viewStart: number; viewEnd: number }
  | { ok: false; message: string };

/**
 * Accepts IGV-like inputs:
 * - `chr1:1000` — center on position
 * - `chr1:1,000-2,000` — explicit range (1-based inclusive start, inclusive end in UI)
 * - `chr1:1000-` — from position to contig end (needs contigLength)
 */
export function parseLocus(
  raw: string,
  defaultContig: string,
  contigLength: number,
): LocusParseResult {
  const text = raw.trim();
  if (!text) return { ok: false, message: "Empty locus" };

  let body = text;
  let contig = defaultContig;

  const colon = text.indexOf(":");
  if (colon >= 0) {
    contig = text.slice(0, colon).trim() || defaultContig;
    body = text.slice(colon + 1).trim();
  }

  if (!contig) return { ok: false, message: "Missing contig" };

  const strip = (s: string) => s.replace(/,/g, "").trim();

  if (!body) {
    return { ok: false, message: "Missing coordinates" };
  }

  const dash = body.indexOf("-");
  if (dash < 0) {
    const pos = Number(strip(body));
    if (!Number.isFinite(pos) || pos < 1) {
      return { ok: false, message: "Invalid position" };
    }
    return centerOn(contig, pos, contigLength);
  }

  const left = strip(body.slice(0, dash));
  const rightRaw = body.slice(dash + 1).trim();
  const start1 = Number(left);
  if (!Number.isFinite(start1) || start1 < 1) {
    return { ok: false, message: "Invalid start" };
  }

  if (!rightRaw) {
    const end0 = contigLength > 0 ? contigLength : start1;
    return {
      ok: true,
      contig,
      viewStart: start1 - 1,
      viewEnd: end0,
    };
  }

  const end1 = Number(strip(rightRaw));
  if (!Number.isFinite(end1) || end1 < start1) {
    return { ok: false, message: "Invalid end" };
  }

  return {
    ok: true,
    contig,
    viewStart: start1 - 1,
    viewEnd: end1,
  };
}

function centerOn(
  contig: string,
  pos1Based: number,
  contigLength: number,
): LocusParseResult {
  const span =
    contigLength > 0
      ? Math.min(50_000, Math.max(100, Math.floor(contigLength / 20)))
      : 500;
  const center0 = pos1Based - 1;
  let viewStart = Math.max(0, center0 - Math.floor(span / 2));
  let viewEnd = viewStart + span;
  if (contigLength > 0) {
    viewEnd = Math.min(contigLength, viewEnd);
    viewStart = Math.max(0, viewEnd - span);
  }
  return { ok: true, contig, viewStart, viewEnd };
}