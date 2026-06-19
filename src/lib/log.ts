import type { LogLevel } from "$lib/types";

export function inferLogLevel(message: string): LogLevel {
  const lowered = message.toLowerCase();
  if (
    lowered.startsWith("error:") ||
    lowered.includes(" error:") ||
    lowered.endsWith(" error") ||
    lowered.includes("failed") ||
    lowered.includes("merge error:") ||
    lowered.includes("alignment error:") ||
    lowered.includes("reference error:")
  ) {
    return "error";
  }
  if (
    lowered.startsWith("warning:") ||
    lowered.startsWith("warn:") ||
    lowered.includes("skipped") ||
    lowered.includes("not found on path")
  ) {
    return "warn";
  }
  return "info";
}