export type Units = "bits" | "bytes";

/** Formats a rate in bytes/s in the user's preferred unit system. */
export function formatRate(bytesPerSec: number, units: Units, compact = false): string {
  if (!isFinite(bytesPerSec) || bytesPerSec < 0) bytesPerSec = 0;
  if (units === "bits") {
    const b = bytesPerSec * 8;
    if (b < 1000) return `${Math.round(b)} bit/s`;
    if (b < 1e6) return `${trim(b / 1e3)} kbit/s`;
    if (b < 1e9) return `${trim(b / 1e6)} Mbit/s`;
    return `${trim(b / 1e9)} Gbit/s`;
  }
  const v = bytesPerSec;
  if (v < 1024) return `${Math.round(v)} B/s`;
  if (v < 1024 ** 2) return `${trim(v / 1024)} KB/s`;
  if (v < 1024 ** 3) return `${trim(v / 1024 ** 2)} MB/s`;
  return `${trim(v / 1024 ** 3, compact)} GB/s`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${trim(bytes / 1024)} KB`;
  if (bytes < 1024 ** 3) return `${trim(bytes / 1024 ** 2)} MB`;
  return `${trim(bytes / 1024 ** 3)} GB`;
}

function trim(n: number, _compact = false): string {
  if (n >= 100) return n.toFixed(0);
  if (n >= 10) return n.toFixed(1);
  return n.toFixed(2);
}

/** Axis tick label: short, no decimals when large. */
export function axisLabel(bytesPerSec: number, units: Units): string {
  if (units === "bits") {
    const b = bytesPerSec * 8;
    if (b < 1e3) return `${Math.round(b)} b`;
    if (b < 1e6) return `${short(b / 1e3)} kb`;
    if (b < 1e9) return `${short(b / 1e6)} Mb`;
    return `${short(b / 1e9)} Gb`;
  }
  if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)} B`;
  if (bytesPerSec < 1024 ** 2) return `${short(bytesPerSec / 1024)} KB`;
  if (bytesPerSec < 1024 ** 3) return `${short(bytesPerSec / 1024 ** 2)} MB`;
  return `${short(bytesPerSec / 1024 ** 3)} GB`;
}

function short(n: number): string {
  return n >= 10 || Number.isInteger(n) ? n.toFixed(0) : n.toFixed(1);
}

/** Rounds a raw maximum up to a "nice" axis maximum in the chosen unit. */
export function niceMax(bytesPerSec: number, units: Units): number {
  const scale = units === "bits" ? 8 : 1;
  const base = units === "bits" ? 1000 : 1024;
  const v = Math.max(bytesPerSec * scale, units === "bits" ? 8000 : 1024);
  let mag = 1;
  while (v / mag >= base) mag *= base;
  const m = v / mag;
  const steps = [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10];
  const nice = steps.find((s) => s >= m) ?? 10;
  return (nice * mag) / scale;
}

export const unitOptions = (units: Units) =>
  units === "bits"
    ? [
        { label: "kbit/s", factor: 1000 / 8 },
        { label: "Mbit/s", factor: 1e6 / 8 },
      ]
    : [
        { label: "KB/s", factor: 1024 },
        { label: "MB/s", factor: 1024 ** 2 },
      ];

export function formatTime(ts: number): string {
  const d = new Date(ts);
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}
