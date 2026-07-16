export const PANE_HANDLE_WIDTH_PX = 14;
/** Allow the input browser to shrink so Tools/View get most of the width. */
export const MIN_FILES_PANE_WIDTH_PX = 140;
export const MIN_RIGHT_COLUMN_WIDTH_PX = 280;
export const MIN_FILES_PANE_PERCENT = 10;
export const MAX_FILES_PANE_PERCENT = 85;
/** Drag left of this width (px) snaps the browser closed. */
export const SNAP_COLLAPSE_FILES_PANE_PX = 96;
/** Restored width when expanding from stash if none saved. */
export const DEFAULT_EXPANDED_FILES_PANE_PERCENT = 28;

export function clampFilesPaneWidth(percent: number, workspaceWidth: number): number {
  if (workspaceWidth <= 0) {
    return percent;
  }

  const maxPercent = Math.min(
    MAX_FILES_PANE_PERCENT,
    ((workspaceWidth - PANE_HANDLE_WIDTH_PX - MIN_RIGHT_COLUMN_WIDTH_PX) / workspaceWidth) *
      100,
  );
  const minPercent = Math.max(
    MIN_FILES_PANE_PERCENT,
    (MIN_FILES_PANE_WIDTH_PX / workspaceWidth) * 100,
  );

  // If the workspace is very narrow, min may exceed max — prefer min room for tools.
  const lo = Math.min(minPercent, maxPercent);
  const hi = Math.max(minPercent, maxPercent);
  return Math.min(hi, Math.max(lo, percent));
}

export function filesPaneWidthPx(percent: number, workspaceWidth: number): number {
  return (percent / 100) * workspaceWidth;
}

/** True when the drag position should snap the files pane closed. */
export function shouldSnapCollapseFilesPane(clientX: number, workspaceLeft: number): boolean {
  return clientX - workspaceLeft < SNAP_COLLAPSE_FILES_PANE_PX;
}
