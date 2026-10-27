import type { JobStatus } from "./api";

export function bytes(n: number): string {
  if (!Number.isFinite(n) || n < 0) return "–";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i++;
  }
  return i === 0 ? `${n} B` : `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}

export function speed(bps: number): string {
  if (!bps || bps < 1) return "–";
  return `${bytes(bps)}/s`;
}

export function mbit(bps: number): string {
  return `${((bps * 8) / 1_000_000).toFixed(bps * 8 >= 100_000_000 ? 0 : 1)} Mbit/s`;
}

export function duration(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(secs)) return "–";
  if (secs < 60) return `${Math.max(1, Math.round(secs))}s`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m}m ${Math.round(secs % 60)}s`;
  const h = Math.floor(m / 60);
  if (h < 48) return `${h}h ${m % 60}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}

export function ago(ms: number): string {
  const d = (Date.now() - ms) / 1000;
  if (d < 45) return "just now";
  if (d < 3600) return `${Math.round(d / 60)} min ago`;
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  return new Date(ms).toLocaleDateString();
}

export function until(ms: number): string {
  const d = (ms - Date.now()) / 1000;
  if (d <= 0) return "expired";
  return `in ${duration(d)}`;
}

export function dateTime(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

export const STATUS_TEXT: Record<JobStatus, string> = {
  queued: "Queued",
  connecting: "Connecting…",
  downloading: "Downloading",
  waiting_peer: "Waiting for sender",
  waiting_direct: "Waiting for direct path",
  reconnecting: "Connection lost, reconnecting",
  paused: "Paused by you",
  verifying: "Verifying data",
  completed: "Complete",
  failed: "Failed",
  canceled: "Canceled",
  needs_attention: "Needs attention",
};

export type Tone = "accent" | "ok" | "warn" | "err" | "muted" | "info" | "out";

export function statusTone(s: JobStatus): Tone {
  switch (s) {
    case "downloading":
    case "verifying":
      return "accent";
    case "completed":
      return "ok";
    case "waiting_peer":
    case "waiting_direct":
    case "reconnecting":
    case "connecting":
      return "info";
    case "needs_attention":
    case "failed":
      return "err";
    default:
      return "muted";
  }
}

export const ACTIVE: JobStatus[] = [
  "queued",
  "connecting",
  "downloading",
  "waiting_peer",
  "waiting_direct",
  "reconnecting",
  "verifying",
];

export function isActive(s: JobStatus) {
  return ACTIVE.includes(s);
}
