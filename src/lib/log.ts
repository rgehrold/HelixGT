import type { LogEntry, LogLevel } from "$lib/types";

export const MAX_LOG_LINES = 500;

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

export function appendLog(entries: LogEntry[], message: string, level?: LogLevel): LogEntry[] {
  const next = [...entries, { level: level ?? inferLogLevel(message), message }];
  return next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next;
}