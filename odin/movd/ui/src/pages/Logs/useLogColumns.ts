import { useEffect, useMemo, useRef, useState } from "react";
import type { LogRow } from "@/lib/types";
import type { ColKey } from "./logic";
import { computeTemplate as _computeTemplate } from "./logic";

export type ColDef = { key: ColKey; label: string; min: number };
const initialCols: ColDef[] = [
  { key: "time",   label: "Time",     min: 160 },
  { key: "level",  label: "Level",    min: 90 },
  { key: "msg",    label: "Message",  min: 240 },
  { key: "ctx",    label: "Context",  min: 140 },
  { key: "source", label: "Source",   min: 200 },
  { key: "fn",     label: "Function", min: 140 },
  { key: "line",   label: "Line",     min: 80  },
];
const DEFAULT_WIDTHS: Record<ColKey, number> = { time: 220, level: 110, msg: 520, ctx: 220, source: 200 };
const STORAGE_KEY_ORDER = "logs_col_order_v1";
const STORAGE_KEY_WIDTHS = "logs_col_widths_v1";

export function useLogColumns(filtered: LogRow[]) {
  const [order, setOrder] = useState<ColKey[]>(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY_ORDER);
      const parsed = raw ? JSON.parse(raw) as ColKey[] : null;
      const valid = parsed && Array.isArray(parsed) && parsed.length===initialCols.length &&
        parsed.every(k => ["time","level","msg","ctx"].includes(k));
      return valid ? parsed : initialCols.map(c=>c.key);
    } catch { return initialCols.map(c=>c.key); }
  });
  const [widths, setWidths] = useState<Record<ColKey, number>>(() => {
    try {
      const raw = localStorage.getItem(STORAGE_KEY_WIDTHS);
      const parsed = raw ? JSON.parse(raw) as Partial<Record<ColKey, number>> : null;
      return { ...DEFAULT_WIDTHS, ...(parsed||{}) } as Record<ColKey, number>;
    } catch { return { ...DEFAULT_WIDTHS }; }
  });
  useEffect(() => { try { localStorage.setItem(STORAGE_KEY_ORDER, JSON.stringify(order)); } catch {} }, [order]);
  useEffect(() => { try { localStorage.setItem(STORAGE_KEY_WIDTHS, JSON.stringify(widths)); } catch {} }, [widths]);

  const resizeInfo = useRef<{ key: ColKey; startX: number; startW: number } | null>(null);
  const onResizeMove = (e: MouseEvent) => {
    const d = resizeInfo.current; if (!d) return;
    const delta = e.clientX - d.startX;
    setWidths(prev => {
      const next = { ...prev } as Record<ColKey, number>;
      const min = initialCols.find(c=>c.key===d.key)!.min;
      next[d.key] = Math.max(min, d.startW + delta);
      return next;
    });
  };
  const onResizeUp = () => {
    resizeInfo.current = null;
    window.removeEventListener("mousemove", onResizeMove);
    window.removeEventListener("mouseup", onResizeUp);
  };
  function onResizeDown(e: React.MouseEvent, key: ColKey) {
    e.preventDefault(); e.stopPropagation();
    resizeInfo.current = { key, startX: e.clientX, startW: widths[key] };
    window.addEventListener("mousemove", onResizeMove);
    window.addEventListener("mouseup", onResizeUp);
  }

  const dragCol = useRef<ColKey | null>(null);
  function onHeaderDragStart(e: React.DragEvent, key: ColKey) {
    dragCol.current = key; e.dataTransfer.setData("text/plain", key); e.dataTransfer.effectAllowed = "move";
  }
  function onHeaderDragOver(e: React.DragEvent) { e.preventDefault(); e.dataTransfer.dropEffect = "move"; }
  function onHeaderDrop(e: React.DragEvent, overKey: ColKey) {
    e.preventDefault();
    const fromKey = (e.dataTransfer.getData("text/plain") as ColKey) || dragCol.current;
    if (!fromKey || fromKey===overKey) return;
    setOrder(prev => {
      const next = prev.slice();
      const fromIdx = next.indexOf(fromKey);
      const toIdx = next.indexOf(overKey);
      if (fromIdx===-1||toIdx===-1) return prev;
      next.splice(fromIdx,1);
      next.splice(toIdx,0,fromKey);
      return next;
    });
    dragCol.current = null;
  }

  function measureTextWidth(samples: string[], extra = 0): number {
    const canvas = document.createElement("canvas");
    const ctx = canvas.getContext("2d");
    if (!ctx) return 200;
    ctx.font = "14px ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial";
    let w=0; for (const s of samples) w=Math.max(w, ctx.measureText(s).width);
    return Math.ceil(w) + extra + 24;
  }
  function autoFitColumn(key: ColKey) {
    const header = initialCols.find(c=>c.key===key)!.label;
    const rows = filtered.slice(0,500);
    const val = (r: any) => key==="time"?r.time
      : key==="level"?r.level
      : key==="msg"?r.msg
      : key==="ctx"?r.ctx
      : key==="source"?r.source
      : key==="fn"?r.fn
      : String(r.line);
    const values = rows.map(val);
    const extra = key === "level" ? 34 : 0;
    const min = initialCols.find(c=>c.key===key)!.min;
    setWidths(prev => ({ ...prev, [key]: Math.max(min, measureTextWidth([header, ...values], extra)) }));
  }
  function onHeaderDoubleClick(e: React.MouseEvent, key: ColKey) { if (e.ctrlKey) autoFitColumn(key); }

  const template = useMemo(() => _computeTemplate(order, widths), [order, widths]);
  function resetColumns() { setOrder(initialCols.map(c=>c.key)); setWidths({ ...DEFAULT_WIDTHS }); }

  useEffect(() => {
    try {
      // console.assert(order.length===initialCols.length, "order length should match");
      // console.assert(order.every(k => widths[k]!==undefined), "every column has width");
    } catch {}
  }, [order, widths]);

  return {
    order, widths, template, onResizeDown,
    onHeaderDragStart, onHeaderDragOver, onHeaderDrop,
    onHeaderDoubleClick, resetColumns
  } as const;
}
export { initialCols };
