import type { AlignmentRead, AnnotationFeature } from "$lib/types";

export type ThemeColors = {
  bg: string;
  border: string;
  text: string;
  muted: string;
  accent: string;
  track: string;
  forward: string;
  reverse: string;
  coverage: string;
  feature: string;
  featureNeg: string;
  selection: string;
};

/** Classic IGV-like palette (flat, limited color). */
export const IGV_FORWARD = "#4a86c7";
export const IGV_REVERSE = "#c84a4a";
export const IGV_COVERAGE = "#6b8cae";
export const IGV_FEATURE = "#9aa3ad";
export const IGV_FEATURE_NEG = "#b89090";
export const IGV_GRAY_READ = "#8a8a8a";
export const IGV_SELECTION = "rgba(74, 134, 199, 0.22)";
export const IGV_OVERVIEW_THUMB = "#4a86c7";

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
  cachedColors = {
    bg: styles.getPropertyValue("--view-canvas-bg").trim() || "#f7f7f7",
    border: styles.getPropertyValue("--view-track-border").trim() || "#cccccc",
    text: styles.getPropertyValue("--text-primary").trim() || "#222222",
    muted: styles.getPropertyValue("--text-muted").trim() || "#666666",
    accent: styles.getPropertyValue("--accent-highlight").trim() || IGV_FORWARD,
    track: styles.getPropertyValue("--view-track-bg").trim() || "#eeeeee",
    forward: IGV_FORWARD,
    reverse: IGV_REVERSE,
    coverage: IGV_COVERAGE,
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

export function baseColor(base: string, muted: string, colorBases: boolean): string {
  if (!colorBases) return muted;
  switch (base.toUpperCase()) {
    case "A":
      return "#2e8b57";
    case "T":
    case "U":
      return "#c44c4c";
    case "G":
      return "#b8860b";
    case "C":
      return "#4169e1";
    case "N":
      return muted;
    default:
      return muted;
  }
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
  if (colorBy === "none") return IGV_GRAY_READ;
  if (colorBy === "mapq") {
    if (r.mapq >= 40) return "#5a9a6e";
    if (r.mapq >= 20) return "#8a8a4a";
    if (r.mapq >= 10) return "#a07050";
    return IGV_GRAY_READ;
  }
  if (colorBy === "pair") {
    if (r.isProperPair) return colors.forward;
    if (r.isPaired) return "#a08060";
    return IGV_GRAY_READ;
  }
  return r.strand === "-" || r.isReverse ? colors.reverse : colors.forward;
}

export function coverageBarColor(
  fraction: number,
  colors: ThemeColors,
  colorMismatches: boolean,
  selected: boolean,
): string {
  if (selected) return "#e8d44d";
  if (!colorMismatches) return colors.coverage;
  if (fraction > 0.25) return "#c84a4a";
  if (fraction > 0.08) return "#c9a227";
  return colors.coverage;
}
