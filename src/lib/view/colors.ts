import type { AlignmentRead, AnnotationFeature } from "$lib/types";

export type ThemeColors = {
  bg: string;
  border: string;
  text: string;
  muted: string;
  accent: string;
  track: string;
  /** Default / uncolored read bars (theme-aware). */
  readGray: string;
  /** Nucleotide letter/tick colors (theme-aware; dark uses neon-on-slate). */
  baseA: string;
  baseT: string;
  baseG: string;
  baseC: string;
  /** Insertion marks on pileup (IGV-like purple). */
  indelIns: string;
  /** Deletion / skip fill on pileup (black or near-black). */
  indelDel: string;
  forward: string;
  reverse: string;
  coverage: string;
  feature: string;
  featureNeg: string;
  selection: string;
};

/** Classic IGV-ish indel colors (fallbacks). */
export const IGV_INDEL_INS = "#c44cff";
export const IGV_INDEL_DEL = "#1a1a1a";

/** Classic IGV-like palette (flat, limited color). */
export const IGV_FORWARD = "#4a86c7";
export const IGV_REVERSE = "#c84a4a";
export const IGV_COVERAGE = "#6b8cae";
export const IGV_FEATURE = "#9aa3ad";
export const IGV_FEATURE_NEG = "#b89090";
/** Fallback when CSS token is missing (prefer theme --view-read-gray). */
export const IGV_GRAY_READ = "#b4b9c2";
export const IGV_SELECTION = "rgba(74, 134, 199, 0.22)";
export const IGV_OVERVIEW_THUMB = "#4a86c7";

/** Light-mode IGV-ish bases (fallbacks). */
const FALLBACK_BASE_A = "#2e8b57";
const FALLBACK_BASE_T = "#c44c4c";
const FALLBACK_BASE_G = "#b8860b";
const FALLBACK_BASE_C = "#4169e1";

let cachedColors: ThemeColors | null = null;
let cacheTs = 0;
const COLOR_CACHE_MS = 2000;

/** Theme colors with short cache (avoids getComputedStyle every paint). */
export function themeColors(force = false): ThemeColors {
  const now = performance.now();
  if (!force && cachedColors && now - cacheTs < COLOR_CACHE_MS) {
    return cachedColors;
  }
  const styles = getComputedStyle(document.documentElement);
  const token = (name: string, fallback: string) =>
    styles.getPropertyValue(name).trim() || fallback;
  cachedColors = {
    bg: token("--view-canvas-bg", "#f7f7f7"),
    border: token("--view-track-border", "#cccccc"),
    text: token("--text-primary", "#222222"),
    muted: token("--text-muted", "#666666"),
    accent: token("--accent-highlight", IGV_FORWARD),
    track: token("--view-track-bg", "#eeeeee"),
    readGray: token("--view-read-gray", IGV_GRAY_READ),
    baseA: token("--view-base-a", FALLBACK_BASE_A),
    baseT: token("--view-base-t", FALLBACK_BASE_T),
    baseG: token("--view-base-g", FALLBACK_BASE_G),
    baseC: token("--view-base-c", FALLBACK_BASE_C),
    indelIns: token("--view-indel-ins", IGV_INDEL_INS),
    indelDel: token("--view-indel-del", IGV_INDEL_DEL),
    forward: IGV_FORWARD,
    reverse: IGV_REVERSE,
    // Neutral grey coverage (allele stacks use base colors when variants are high).
    coverage: token("--view-coverage-gray", token("--view-read-gray", IGV_GRAY_READ)),
    feature: IGV_FEATURE,
    featureNeg: IGV_FEATURE_NEG,
    selection: IGV_SELECTION,
  };
  cacheTs = now;
  return cachedColors;
}

export function invalidateThemeColorCache() {
  cachedColors = null;
  cacheTs = 0;
}

function parseHexRgb(hex: string): [number, number, number] | null {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1]!, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** Theme-aware nucleotide RGB (light: classic IGV; dark: neon-on-slate). */
export function baseRgb(base: string): [number, number, number] | null {
  const c = themeColors();
  switch (base.toUpperCase()) {
    case "A":
      return parseHexRgb(c.baseA) ?? parseHexRgb(FALLBACK_BASE_A);
    case "T":
    case "U":
      return parseHexRgb(c.baseT) ?? parseHexRgb(FALLBACK_BASE_T);
    case "G":
      return parseHexRgb(c.baseG) ?? parseHexRgb(FALLBACK_BASE_G);
    case "C":
      return parseHexRgb(c.baseC) ?? parseHexRgb(FALLBACK_BASE_C);
    default:
      return null;
  }
}

export function baseColor(base: string, muted: string, colorBases: boolean): string {
  if (!colorBases) return muted;
  const rgb = baseRgb(base);
  if (!rgb) return muted;
  return `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
}

/**
 * Map Phred quality → letter/rect opacity (IGV-style: low Q = more transparent).
 * Q≈5 → ~0.25, Q≈20 → ~0.57, Q≈40 → 1.0
 */
export function qualityToAlpha(phred: number): number {
  const t = Math.max(0, Math.min(1, (phred - 5) / 35));
  return 0.25 + t * 0.75;
}

/** Base letter/bar color with quality-based alpha. */
export function baseColorWithQuality(
  base: string,
  phred: number,
  fallback: string,
): string {
  const rgb = baseRgb(base);
  const a = qualityToAlpha(phred);
  if (!rgb) {
    // N / unknown — muted gray with same alpha curve
    return `rgba(148, 163, 184, ${a.toFixed(3)})`;
  }
  return `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, ${a.toFixed(3)})`;
}

export function featureColor(f: AnnotationFeature, colors: ThemeColors): string {
  if (f.strand === "-") return colors.featureNeg;
  if (f.strand === "+") return colors.feature;
  return colors.feature;
}

export function readFillColor(
  r: AlignmentRead,
  selected: boolean,
  colorBy: "strand" | "mapq" | "pair" | "none",
  colors: ThemeColors,
): string {
  if (selected) return "#e8d44d";
  if (colorBy === "none") return colors.readGray;
  if (colorBy === "mapq") {
    if (r.mapq >= 40) return "#5a9a6e";
    if (r.mapq >= 20) return "#8a8a4a";
    if (r.mapq >= 10) return "#a07050";
    return colors.readGray;
  }
  if (colorBy === "pair") {
    if (r.isProperPair) return colors.forward;
    if (r.isPaired) return "#a08060";
    return colors.readGray;
  }
  return r.strand === "-" || r.isReverse ? colors.reverse : colors.forward;
}

/** Solid coverage bar fill (allele stacks are painted separately). */
export function coverageBarColor(
  _fraction: number,
  colors: ThemeColors,
  _colorMismatches: boolean,
  selected: boolean,
): string {
  if (selected) return "#e8d44d";
  return colors.coverage;
}
