export const PANE_HANDLE_WIDTH_PX = 22;
export const MIN_FILES_PANE_WIDTH_PX = 300;
export const MIN_RIGHT_COLUMN_WIDTH_PX = 280;
export const MIN_FILES_PANE_PERCENT = 32;
export const MAX_FILES_PANE_PERCENT = 78;

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

  return Math.min(maxPercent, Math.max(minPercent, percent));
}