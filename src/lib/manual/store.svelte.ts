/** Shared state for the built-in user manual. */
export const manualState = $state({
  open: false,
  pageId: "welcome" as string,
});

export function openManual(pageId?: string) {
  if (pageId) manualState.pageId = pageId;
  manualState.open = true;
}

export function closeManual() {
  manualState.open = false;
}

export function setManualPage(pageId: string) {
  manualState.pageId = pageId;
  if (!manualState.open) manualState.open = true;
}
