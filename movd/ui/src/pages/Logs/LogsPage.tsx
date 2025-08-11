// src/pages/Logs/LogsPage.tsx
import React, { useEffect, useMemo, useRef, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@/components/ui/select";
import { ArrowUpDown, ArrowUp, ArrowDown, Download, Maximize, RefreshCcw } from "lucide-react";

import type { LogRow } from "@/lib/types";
import { generateLogs } from "@/lib/mock";

import {
  ColumnDef,
  getCoreRowModel,
  getSortedRowModel,
  SortingState,
  flexRender,
  useReactTable,
  ColumnOrderState,
  ColumnResizeMode,
  Column as TableColumn,
} from "@tanstack/react-table";
import { useVirtualizer } from "@tanstack/react-virtual";

import {
  DndContext,
  PointerSensor,
  KeyboardSensor,
  useSensor,
  useSensors,
  DragOverlay,
  closestCenter,
} from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  arrayMove,
  horizontalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";

// -----------------------------------------------------------------------------
// helpers
// -----------------------------------------------------------------------------

type ColKey = "time" | "level" | "msg" | "ctx" | "source" | "fn" | "line";

const DEFAULT_WIDTH: Record<ColKey, number> = {
  time: 220,
  level: 110,
  msg: 520,
  ctx: 180,
  source: 320,
  fn: 200,
  line: 90,
};

function SortGlyph({ active, dir }: { active: boolean; dir: "asc" | "desc" | null }) {
  if (!active || !dir) return <ArrowUpDown className="w-3.5 h-3.5 opacity-60" />;
  return dir === "asc" ? (
    <ArrowUp className="w-3.5 h-3.5 opacity-80" />
  ) : (
    <ArrowDown className="w-3.5 h-3.5 opacity-80" />
  );
}

function safeFilter(rows: LogRow[], q: string, level: string) {
  const query = q.trim().toLowerCase();
  const byLevel = level === "ALL" ? rows : rows.filter((r) => r.level === level);
  if (!query) return byLevel;
  return byLevel.filter((r) => {
    const hay = `${r.time} ${r.level} ${r.msg} ${r.ctx ?? ""} ${r.source ?? ""} ${r.fn ?? ""} ${r.line ?? ""}`.toLowerCase();
    return hay.includes(query);
  });
}

const headerClasses =
  "text-zinc-400 bg-zinc-900/90 backdrop-blur supports-[backdrop-filter]:bg-zinc-900/70";

// -----------------------------------------------------------------------------
// page
// -----------------------------------------------------------------------------

export default function LogsPage() {
  // filters/sort
  const [q, setQ] = useState("");
  const [level, setLevel] = useState<string>("ALL");
  const [sorting, setSorting] = useState<SortingState>([]);
  const [columnResizeMode] = useState<ColumnResizeMode>("onChange");

  // data
  const logs = useMemo(() => generateLogs(), []);
  const filtered = useMemo(() => safeFilter(logs, q, level), [logs, q, level]);

  // column defs
  const columns = useMemo<ColumnDef<LogRow, any>[]>(() => {
    return [
      {
        id: "time",
        accessorKey: "time",
        header: () => <span>Time</span>,
        cell: (ctx) => <span className="font-mono text-xs text-zinc-400">{ctx.getValue<string>()}</span>,
        size: DEFAULT_WIDTH.time,
        minSize: 160,
      },
      {
        id: "level",
        accessorKey: "level",
        header: () => <span>Level</span>,
        cell: (ctx) => {
          const lvl = ctx.getValue<string>();
          const cls =
            lvl === "ERROR" ? "bg-red-900" :
            lvl === "WARN"  ? "bg-amber-900" :
            lvl === "DEBUG" ? "bg-zinc-700" : "bg-emerald-900";
          return <Badge className={cls}>{lvl}</Badge>;
        },
        size: DEFAULT_WIDTH.level,
        minSize: 90,
      },
      {
        id: "msg",
        accessorKey: "msg",
        header: () => <span>Message</span>,
        cell: (ctx) => (
          <span className="whitespace-pre-wrap break-words break-all leading-relaxed">
            {ctx.getValue<string>()}
          </span>
        ),
        size: DEFAULT_WIDTH.msg,
        minSize: 240,
      },
      {
        id: "ctx",
        accessorKey: "ctx",
        header: () => <span>Context</span>,
        cell: (ctx) => <span className="text-zinc-400">{ctx.getValue<string>()}</span>,
        size: DEFAULT_WIDTH.ctx,
        minSize: 140,
      },
      {
        id: "source",
        accessorKey: "source",
        header: () => <span>Source</span>,
        cell: (ctx) => <span className="text-zinc-200 font-mono text-xs">{ctx.getValue<string>()}</span>,
        size: DEFAULT_WIDTH.source,
        minSize: 220,
      },
      {
        id: "fn",
        accessorKey: "fn",
        header: () => <span>Function</span>,
        cell: (ctx) => <span className="text-zinc-100">{ctx.getValue<string>()}</span>,
        size: DEFAULT_WIDTH.fn,
        minSize: 140,
      },
      {
        id: "line",
        accessorKey: "line",
        header: () => <span>Line</span>,
        cell: (ctx) => <span className="text-zinc-400 font-mono text-xs">{ctx.getValue<number>()}</span>,
        size: DEFAULT_WIDTH.line,
        minSize: 80,
      },
    ];
  }, []);

  // table state (controlled sizing!)
  const [columnOrder, setColumnOrder] = useState<ColumnOrderState>(columns.map((c) => c.id as string));
  const [columnSizing, setColumnSizing] = useState<Record<string, number>>({});
  const [columnSizingInfo, setColumnSizingInfo] = useState<any>({});

  function setColSize(c: TableColumn<any, unknown>, nextSize: number) {
    const id = c.id;
    setColumnSizing(prev => ({
      ...prev,
      [id]: Math.max(minOf(c), Math.round(nextSize)),
    }));
  }


  const table = useReactTable({
    data: filtered,
    columns,
    state: {
      sorting,
      columnOrder,
      columnSizing,
      columnSizingInfo,
    },
    onSortingChange: setSorting,
    onColumnOrderChange: setColumnOrder,
    onColumnSizingChange: setColumnSizing,
    onColumnSizingInfoChange: setColumnSizingInfo,
    columnResizeMode,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  // scroll + virtualization
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const rows = table.getRowModel().rows;
  const totalWidth = table.getTotalSize();

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 36,
    overscan: 12,
    getItemKey: (index) => rows[index].id,
  });

  // measure each row element directly
  const registerRow = (el: HTMLElement | null) => {
    if (el) rowVirtualizer.measureElement(el);
  };

  // per-row wrap detection → top-align wrapped rows
  const msgRefs = useRef<Record<number, HTMLSpanElement | null>>({});
  const [wrappedRows, setWrappedRows] = useState<Set<number>>(new Set());
  const visibleKey = useMemo(
    () => rowVirtualizer.getVirtualItems().map((v) => v.index).join(","),
    [rowVirtualizer.getVirtualItems()]
  );
  useEffect(() => {
    const next = new Set<number>();
    for (const v of rowVirtualizer.getVirtualItems()) {
      const el = msgRefs.current[v.index];
      if (!el) continue;
      const lh = parseFloat(getComputedStyle(el).lineHeight || "16");
      if (lh > 0 && el.scrollHeight > lh * 1.5) next.add(v.index);
    }
    let changed = next.size !== wrappedRows.size;
    if (!changed) for (const i of next) if (!wrappedRows.has(i)) { changed = true; break; }
    if (changed) setWrappedRows(next);
  }, [visibleKey, totalWidth]);

  // freeze vertical scroll while dragging headers
  const [isDraggingHeader, setIsDraggingHeader] = useState(false);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el || !isDraggingHeader) return;
    const prevent = (e: Event) => e.preventDefault();
    el.addEventListener("wheel", prevent, { passive: false });
    el.addEventListener("touchmove", prevent, { passive: false });
    return () => {
      el.removeEventListener("wheel", prevent as any);
      el.removeEventListener("touchmove", prevent as any);
    };
  }, [isDraggingHeader]);
  useEffect(() => {
    const clear = () => setIsDraggingHeader(false);
    window.addEventListener("dragend", clear, true);
    window.addEventListener("drop", clear, true);
    return () => {
      window.removeEventListener("dragend", clear, true);
      window.removeEventListener("drop", clear, true);
    };
  }, []);

  // dnd-kit sensors/state for header drag
  const [activeColId, setActiveColId] = useState<string | null>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 6 } }),
    useSensor(KeyboardSensor)
  );

  // resize vs drag conflict guard
  const [activeResizing, setActiveResizing] = useState(false);
  useEffect(() => {
    const stop = () => setActiveResizing(false);
    window.addEventListener("mouseup", stop, true);
    window.addEventListener("touchend", stop, true);
    return () => {
      window.removeEventListener("mouseup", stop, true);
      window.removeEventListener("touchend", stop, true);
    };
  }, []);

  // drop-zone side highlight (optional)
  const [dragOverId, setDragOverId] = useState<string | null>(null);
  const [insertSide, setInsertSide] = useState<"left" | "right" | null>(null);

  // --- Auto Fit (msg takes remainder; others shrink to min as needed) ---
  const [autoFitOnResize, setAutoFitOnResize] = useState(true);

  function minOf(c: TableColumn<any, unknown>) {
    return (typeof c.columnDef.minSize === "number" ? c.columnDef.minSize : 50) as number;
  }

  function applyAutoFit() {
    const scroller = scrollRef.current;
    if (!scroller) return;

    const viewport = scroller.clientWidth; // visible width excluding vertical scrollbar
    const cols = table.getVisibleLeafColumns();
    if (!cols.length) return;

    const msg = cols.find((c) => c.id === "msg");
    if (!msg) return;

    const others = cols.filter((c) => c.id !== "msg");
    const sumOthers = others.reduce((s, c) => s + c.getSize(), 0);
    const sumAll = sumOthers + msg.getSize();

    if (Math.abs(sumAll - viewport) < 1) return; // already fits visually

    if (sumAll > viewport) {
      // Need to shrink others down toward min first
      let need = sumAll - viewport;

      const shrinkables = others.map((c) => ({
        c,
        can: Math.max(0, c.getSize() - minOf(c)),
      }));
      let totalCan = shrinkables.reduce((s, x) => s + x.can, 0);

      if (totalCan > 0) {
        // shrink proportionally until we cover "need" or hit mins
        shrinkables.forEach((x, idx) => {
          if (need <= 0) return;
          // proportion of remaining need
          const share =
            idx === shrinkables.length - 1
              ? need
              : Math.min(need, Math.floor((x.can / totalCan) * need));
          const next = Math.max(minOf(x.c), x.c.getSize() - share);
          const used = x.c.getSize() - next;
          setColSize(x.c, next);
          need -= used;
        });
      }

      // whatever remains must come from msg, but never below its min
      if (need > 0) {
        const mMin = minOf(msg);
        const take = Math.min(need, Math.max(0, msg.getSize() - mMin));
        setColSize(msg, msg.getSize() - take);
        need -= take;
      }
      // if still need > 0, viewport is too small; we accept h-scroll
    } else {
      // We have extra space: give it all to msg
      const gain = viewport - sumAll;
      setColSize(msg, msg.getSize() + gain);
    }

    // Encourage a layout pass this frame (reads the new total)
    void table.getTotalSize();
  }

  // Re-apply auto fit on container width changes when enabled
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      if (autoFitOnResize) applyAutoFit();
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [autoFitOnResize, table]);

  const leafHeaders = table.getHeaderGroups().at(-1)!.headers;
  const leafHeaderIds = leafHeaders.map((h) => h.column.id);

  const dndVersion = leafHeaderIds.join("|");

  useEffect(() => {
    // Column order changed → ensure dnd state is clean
    setActiveColId(null);
    setDragOverId(null);
    setInsertSide(null);
  }, [columnOrder, leafHeaderIds.join("|")]);

  const scrollContainerRef = useRef<HTMLDivElement>(null);
  const [savedOverflowY, setSavedOverflowY] = useState<string | null>(null);

  useEffect(() => {
    const sc = scrollContainerRef.current;
    if (!sc) return;

    if (isDraggingHeader) {
      // Save original vertical overflow and lock it
      setSavedOverflowY(sc.style.overflowY);
      sc.style.overflowY = "hidden";
    } else if (savedOverflowY !== null) {
      // Restore vertical overflow
      sc.style.overflowY = savedOverflowY;
      setSavedOverflowY(null);
    }
  }, [isDraggingHeader, savedOverflowY]);

  useEffect(() => {
    if (!columnOrder.length) {
      const seed = table.getAllLeafColumns().map(c => c.id);
      if (seed.length) setColumnOrder(seed);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [table, columnOrder.length]);

  function resetColumnsAndFit() {
    // Clear sizing to defaults
    setColumnSizing({});
    setColumnSizingInfo({});

    // New order = current visible leaf columns (stable ids)
    const freshOrder = table.getAllLeafColumns().map((c) => c.id);
    setColumnOrder(freshOrder);

    // Clear any DnD hover state
    setDragOverId(null);
    setInsertSide(null);
    setActiveColId(null);

    // Re-apply Auto fit if desired
    applyAutoFit();
  }

  return (
    <div className="flex flex-col min-h-0 h-full gap-4">
      {/* Controls */}
      <Card className="bg-zinc-900/60 border-zinc-800">
        <CardHeader className="flex flex-row items-center justify-between gap-2">
          <CardTitle>Search Logs</CardTitle>
          <div className="flex items-center gap-2">
            <Button variant="outline" className="border-zinc-800" onClick={applyAutoFit}>
              <Maximize className="w-4 h-4" /> Auto fit
            </Button>
            <Button
              variant="outline"
              className="border-zinc-800 whitespace-nowrap data-[state=on]:bg-emerald-600 data-[state=on]:text-zinc-50"
              data-state={autoFitOnResize ? "on" : "off"}
              aria-pressed={autoFitOnResize}
              onClick={() => setAutoFitOnResize(v => !v)}
              title="When enabled, re-apply Auto fit whenever the table container resizes."
            >
              <RefreshCcw className="w-4 h-4" />
              Auto fit on resize
            </Button>
            <Button
              variant="outline"
              className="border-zinc-800"
              onClick={resetColumnsAndFit}
            >
              <ArrowUpDown className="w-4 h-4" /> Reset columns
            </Button>
            <Button variant="outline" className="border-zinc-800">
              <Download className="w-4 h-4" /> Export results
            </Button>
          </div>
        </CardHeader>
        <CardContent className="grid gap-3">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-2">
            <Input
              placeholder="Search message, context, source, function…"
              value={q}
              onChange={(e) => setQ(e.target.value)}
              className="bg-zinc-950 border-zinc-800"
            />
            <Select value={level} onValueChange={setLevel}>
              <SelectTrigger className="w-full bg-zinc-950 border-zinc-800 focus-visible:ring-2 focus-visible:ring-emerald-600 focus-visible:ring-offset-2 focus-visible:ring-offset-background">
                <SelectValue />
              </SelectTrigger>
              <SelectContent className="border-zinc-800 bg-zinc-950 focus-visible:ring-2 focus-visible:ring-emerald-600 focus-visible:ring-offset-2 focus-visible:ring-offset-background">
                <SelectItem value="ALL">ALL</SelectItem>
                <SelectItem value="DEBUG">DEBUG</SelectItem>
                <SelectItem value="INFO">INFO</SelectItem>
                <SelectItem value="WARN">WARN</SelectItem>
                <SelectItem value="ERROR">ERROR</SelectItem>
              </SelectContent>
            </Select>
            <div />
          </div>
        </CardContent>
      </Card>

      {/* Table */}
      <div className="rounded-2xl border border-zinc-800 overflow-hidden flex-1 min-h-0">
        <div
          ref={scrollRef}
          className="relative h-full logs-scroll"
          style={{
            overflowX: "auto",
            overflowY: isDraggingHeader ? "auto" : "auto",
          }}
        >
          {/* Sticky header */}
          <div className="sticky top-0 z-10">
            <DndContext
              key={`dnd-${dndVersion}`}
              autoScroll={false}
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragStart={({ active }) => {
                setIsDraggingHeader(true);
                setActiveColId(String(active.id));
              }}
              onDragCancel={() => {
                setIsDraggingHeader(false);
                setActiveColId(null);
                setDragOverId(null);
                setInsertSide(null);
              }}
              onDragEnd={({ active, over, delta }) => {
                setIsDraggingHeader(false);
                setActiveColId(null);

                if (!over) { setDragOverId(null); setInsertSide(null); return; }

                const srcId = String(active.id);
                const dstId = String(over.id);

                setColumnOrder((prev) => {
                  if (srcId === dstId) return prev;

                  // Indices in the current order (before removal)
                  const fromPrev = prev.indexOf(srcId);
                  const toPrev   = prev.indexOf(dstId);
                  if (fromPrev === -1 || toPrev === -1) return prev;

                  // Decide side: prefer insertSide, then delta.x, then rects, then index inference
                  let dropOnRight: boolean | null = null;
                  if (insertSide) {
                    dropOnRight = insertSide === "right";
                  } else if (typeof delta?.x === "number" && Math.abs(delta.x) > 0) {
                    dropOnRight = delta.x > 0;
                  } else {
                    const aRect = (active.rect?.current?.translated ?? active.rect?.current) as { center?: { x: number } } | undefined;
                    const oRect = (over as any)?.rect as { left: number; width: number } | undefined;
                    if (aRect?.center?.x != null && oRect?.left != null && oRect?.width != null) {
                      dropOnRight = aRect.center.x >= oRect.left + oRect.width / 2;
                    }
                  }
                  if (dropOnRight == null) {
                    dropOnRight = fromPrev < toPrev; // left→right => after, right→left => before
                  }

                  const next = [...prev];
                  const [moved] = next.splice(fromPrev, 1);   // remove first
                  let to = next.indexOf(dstId);               // re-find target after removal
                  if (to === -1) return prev;
                  if (dropOnRight) to += 1;                   // insert after if right side
                  to = Math.max(0, Math.min(to, next.length));
                  next.splice(to, 0, moved);
                  return next;
                });

                setDragOverId(null);
                setInsertSide(null);
                if (autoFitOnResize) applyAutoFit();
              }}
            >
              <SortableContext key={`ctx-${dndVersion}`} items={leafHeaderIds} strategy={horizontalListSortingStrategy}>
                <div className="flex select-none" style={{ width: totalWidth }}>
                  {leafHeaders.map((header) => (
                    <SortableHeaderCell
                      key={header.column.id}
                      header={header}
                      headerClasses={headerClasses}
                      sorting={sorting}
                      setSorting={setSorting}
                      activeResizing={activeResizing}
                      setActiveResizing={setActiveResizing}
                      setInsertSide={setInsertSide}
                      setDragOverId={setDragOverId}
                      dragOverId={dragOverId}
                      insertSide={insertSide}
                    />
                  ))}
                </div>
              </SortableContext>
              <DragOverlay dropAnimation={null} />
            </DndContext>
          </div>

          {/* Virtualized rows */}
          <div style={{ height: rowVirtualizer.getTotalSize(), position: "relative" }}>
            {rowVirtualizer.getVirtualItems().map((vr) => {
              const row = rows[vr.index];
              const isWrappedRow = wrappedRows.has(vr.index);
              return (
                <div
                  key={row.id}
                  data-index={vr.index}
                  ref={registerRow}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${vr.start}px)`,
                  }}
                >
                  <div
                    className={`flex ${isWrappedRow ? "items-start pt-1" : "items-center py-1"} border-b border-zinc-800`}
                    style={{ width: totalWidth }}
                  >
                    {row.getVisibleCells().map((cell) => {
                      const id = cell.column.id;
                      const size = cell.column.getSize();
                      return (
                        <div
                          key={cell.id}
                          style={{
                            flex: id === "msg" ? `1 1 ${Math.max(240, size)}px` : `0 0 ${size}px`,
                            minWidth: id === "msg" ? Math.max(240, size) : size,
                          }}
                          className="px-3"
                        >
                          {id === "msg" ? (
                            <span
                              ref={(el) => { msgRefs.current[vr.index] = el; }}
                              className="whitespace-pre-wrap break-words break-all leading-relaxed"
                            >
                              {flexRender(cell.column.columnDef.cell, cell.getContext())}
                            </span>
                          ) : (
                            <div className="flex items-center">
                              {flexRender(cell.column.columnDef.cell, cell.getContext())}
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

// -----------------------------------------------------------------------------
// Sortable header cell (dnd-kit). Resize wins; sort stops propagation.
// -----------------------------------------------------------------------------

function SortableHeaderCell({
  header,
  headerClasses,
  sorting,
  setSorting,
  activeResizing,
  setActiveResizing,
  setInsertSide,
  setDragOverId,
  dragOverId,
  insertSide,
}: {
  header: any;
  headerClasses: string;
  sorting: any[];
  setSorting: (updater: any) => void;
  activeResizing: boolean;
  setActiveResizing: (v: boolean) => void;
  setInsertSide: (v: "left" | "right" | null) => void;
  setDragOverId: (v: string | null) => void;
  dragOverId: string | null;
  insertSide: "left" | "right" | null;
}) {
  const col = header.column;
  const id = col.id as string;
  const size = col.getSize();
  const sortEntry = sorting.find((s) => s.id === id);

  const { setNodeRef, attributes, listeners, transform, transition, isOver } =
    useSortable({ id, disabled: activeResizing });

  const style: React.CSSProperties = {
    transform: CSS.Translate.toString(transform),
    transition,
    flex: id === "msg" ? `1 1 ${Math.max(240, size)}px` : `0 0 ${size}px`,
    minWidth: id === "msg" ? Math.max(240, size) : size,
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isOver) return;
    const rect = e.currentTarget.getBoundingClientRect();
    setInsertSide(e.clientX < rect.left + rect.width / 2 ? "left" : "right");
    setDragOverId(id);
  };

  const showLeft = (isOver || dragOverId === id) && insertSide === "left";
  const showRight = (isOver || dragOverId === id) && insertSide === "right";

  const tsResize = header.getResizeHandler();

  return (
    <div
      ref={setNodeRef}
      {...attributes}
      {...(!activeResizing ? listeners : {})} // attach dnd listeners to the cell
      className={`relative text-sm font-medium cursor-default ${headerClasses} ${
        isOver ? "bg-zinc-800/50" : ""
      } border-r border-zinc-800`}
      style={style}
      onPointerMove={onPointerMove}
      onPointerLeave={() => {
        setDragOverId(null);
        setInsertSide(null);
      }}
      onDoubleClick={() => header.column.resetSize()}
      title="Drag to reorder • Double‑click to auto‑fit • Click to sort"
    >
      {/* insertion guides */}
      <div className={`absolute top-0 bottom-0 left-0 w-[2px] ${showLeft ? "bg-emerald-600" : "bg-transparent"}`} />
      <div className={`absolute top-0 bottom-0 right-0 w-[2px] ${showRight ? "bg-emerald-600" : "bg-transparent"}`} />

      {/* sort button (don’t start drag from here) */}
      <button
        type="button"
        draggable={false}
        onMouseDown={(e) => e.stopPropagation()}
        onClick={() => {
          if (!sortEntry) setSorting([{ id, desc: false }]);
          else if (!sortEntry.desc) setSorting([{ id, desc: true }]);
          else setSorting([]);
        }}
        className="sort-button w-full flex items-center justify-between px-3 py-2"
      >
        <span className="truncate">
          {flexRender(header.column.columnDef.header, header.getContext())}
        </span>
        <SortGlyph active={!!sortEntry} dir={sortEntry ? (sortEntry.desc ? "desc" : "asc") : null} />
      </button>

      {/* resize handle — wins over drag (use *capture* to block dnd-kit immediately) */}
      {header.column.getCanResize() && (
        <div
          role="separator"
          aria-orientation="vertical"
          className="resize-handle absolute top-0 right-0 h-full w-[10px] cursor-col-resize z-50"
          style={{ touchAction: "none" }}
          onPointerDownCapture={(e) => {
            // stop dnd-kit before it sees the pointer
            setActiveResizing(true);
            e.stopPropagation();
          }}
          onMouseDown={(e) => {
            // forward to TanStack resizer
            header.getResizeHandler()(e);
          }}
          onTouchStart={(e) => {
            header.getResizeHandler()(e as any);
          }}
        />
      )}
    </div>
  );
}
