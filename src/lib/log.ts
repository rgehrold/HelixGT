import type { LogEntry, LogLevel } from "$lib/types";

export const MAX_LOG_LINES = 800;

export function inferLogLevel(message: string): LogLevel {
  const lowered = message.toLowerCase();
  if (
    lowered.startsWith("error:") ||
    lowered.includes(" error:") ||
    lowered.endsWith(" error") ||
    lowered.includes("failed") ||
    lowered.includes("merge error:") ||
    lowered.includes("alignment error:") ||
    lowered.includes("reference error:") ||
    lowered.includes("view open failed")
  ) {
    return "error";
  }
  if (
    lowered.startsWith("warning:") ||
    lowered.startsWith("warn:") ||
    lowered.includes("skipped") ||
    lowered.includes("not found on path") ||
    lowered.includes("cancelled")
  ) {
    return "warn";
  }
  return "info";
}

function timestamp(): string {
  const d = new Date();
  const hh = String(d.getHours()).padStart(2, "0");
  const mm = String(d.getMinutes()).padStart(2, "0");
  const ss = String(d.getSeconds()).padStart(2, "0");
  return `${hh}:${mm}:${ss}`;
}

export function appendLog(entries: LogEntry[], message: string, level?: LogLevel): LogEntry[] {
  const next = [
    ...entries,
    {
      level: level ?? inferLogLevel(message),
      message,
      time: timestamp(),
    },
  ];
  return next.length > MAX_LOG_LINES ? next.slice(next.length - MAX_LOG_LINES) : next;
}

export function formatLogLine(entry: LogEntry): string {
  const prefix = entry.time ? `[${entry.time}] ` : "";
  const tag =
    entry.level === "error" ? "ERROR" : entry.level === "warn" ? "WARN" : "INFO";
  return `${prefix}${tag}  ${entry.message}`;
}

export function logsToText(entries: LogEntry[]): string {
  return entries.map(formatLogLine).join("\n");
}
