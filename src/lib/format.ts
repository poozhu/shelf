export function formatClock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function dayLabel(ms: number): string {
  const d = new Date(ms);
  const now = new Date();
  if (isSameDay(d, now)) return "今天";
  if (isSameDay(d, new Date(now.getTime() - 86_400_000))) return "昨天";
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** 路径太长时只保留末尾若干段 */
export function shortPath(p: string, keep = 2): string {
  const sep = p.includes("\\") ? "\\" : "/";
  const parts = p.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= keep + 1) return p;
  return "…" + sep + parts.slice(-keep).join(sep);
}

function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}
