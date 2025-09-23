// src/pages/Logs/logic.ts
import type { LogRow } from "@/lib/types";
export type ColKey = "time" | "level" | "msg" | "ctx" | "source" | "fn" | "line";
export const DEFAULT_WIDTHS: Record<ColKey, number> = {
  time: 220, level: 110, msg: 520, ctx: 180, source: 280, fn: 180, line: 90
};

export function computeTemplate(order: ColKey[], widths: Record<ColKey, number>): string {
  return order.map(k => {
    if (k === "msg") return `minmax(${Math.max(160, widths[k] ?? 240)}px, 1fr)`; // flex column
    return `${Math.max(60, widths[k] ?? 120)}px`;
  }).join(" ");
}

export function reorder(order: ColKey[], from: ColKey, to: ColKey): ColKey[] {
  if (from === to) return order.slice();
  const next = order.slice();
  const fi = next.indexOf(from), ti = next.indexOf(to);
  if (fi < 0 || ti < 0) return order.slice();
  next.splice(fi, 1); next.splice(ti, 0, from);
  return next;
}

export function safeFilter(rows: LogRow[], q: string, level: string|""): LogRow[] {
  const qq = (q||"").toLowerCase();
  return rows.filter(r => {
    const levelOk = !level || r.level === level;
    const hay = `${r.msg} ${r.ctx} ${r.source} ${r.fn}`.toLowerCase();
    return levelOk && (!qq || hay.includes(qq));
  });
}

export type SortKey = ColKey;
export type SortDir = "asc" | "desc" | null;

export function sortRows(rows: LogRow[], key: SortKey, dir: SortDir): LogRow[] {
  if (!dir) return rows;
  const sign = dir === "asc" ? 1 : -1;
  const cmp = (a: LogRow, b: LogRow) => {
    let av: string | number = "", bv: string | number = "";
    switch (key) {
      case "time":  av = a.time;  bv = b.time; break;
      case "level": av = a.level; bv = b.level; break;
      case "msg":   av = a.msg;   bv = b.msg; break;
      case "ctx":   av = a.ctx;   bv = b.ctx; break;
      case "source":av = a.source;bv = b.source; break;
      case "fn":    av = a.fn;    bv = b.fn; break;
      case "line":  av = a.line;  bv = b.line; break;
    }
    if (typeof av === "number" && typeof bv === "number") return (av - bv) * sign;
    return String(av).localeCompare(String(bv), undefined, { numeric: true }) * sign;
  };
  return [...rows].sort(cmp);
}
