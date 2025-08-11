// src/pages/Schemas.tsx
import React, { useMemo, useRef, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { renderDDL, importDDL } from "@/lib/utils";
import type { TableDef, Column } from "@/lib/types";
import { Trash2, Plus, Copy, Upload, GripVertical } from "lucide-react";

const TYPE_OPTS = [
  "int",
  "bigint",
  "uuid",
  "text",
  "varchar(255)",
  "boolean",
  "timestamp",
  "timestamptz",
  "jsonb",
  "float",
  "double precision",
];

type SavedSchema = { name: string; columns: Column[] };

export default function Schemas() {
  // ----- Schema Selector (restored) -----
  const [schemas, setSchemas] = useState<SavedSchema[]>([
    {
      name: "users",
      columns: [
        { name: "id", type: "uuid", nullable: false },
        { name: "email", type: "text", nullable: false },
        { name: "created_at", type: "timestamptz", nullable: false, default: "now()" },
      ],
    },
    {
      name: "orders",
      columns: [
        { name: "id", type: "uuid", nullable: false },
        { name: "user_id", type: "uuid", nullable: false },
        { name: "total", type: "double precision", nullable: false, default: "0" },
      ],
    },
  ]);
  const [activeIdx, setActiveIdx] = useState(0);

  // working copy edited by the builder
  const [name, setName] = useState(schemas[activeIdx].name);
  const [cols, setCols] = useState<Column[]>(schemas[activeIdx].columns);

  // switch schema -> load into working copy
  React.useEffect(() => {
    setName(schemas[activeIdx].name);
    setCols(schemas[activeIdx].columns);
  }, [activeIdx]); // eslint-disable-line

  function saveSchema() {
    setSchemas((prev) => prev.map((s, i) => (i === activeIdx ? { name, columns: cols } : s)));
  }
  function addSchema() {
    setSchemas((prev) => [
      ...prev,
      { name: `schema_${prev.length + 1}`, columns: [{ name: "id", type: "uuid", nullable: false }] },
    ]);
    setActiveIdx(schemas.length);
  }
  function deleteSchema(i: number) {
    if (schemas.length === 1) return; // keep at least one
    setSchemas((prev) => prev.filter((_, ix) => ix !== i));
    setActiveIdx(i > 0 ? i - 1 : 0);
  }

  // ----- Table Builder (restored) -----
  const table = useMemo<TableDef>(() => ({ name, columns: cols }), [name, cols]);
  const ddl = useMemo(() => renderDDL(table), [table]);
  const [msg, setMsg] = useState<string>("");

  function flash(t: string) {
    setMsg(t);
    window.setTimeout(() => setMsg(""), 1400);
  }

  function addCol() {
    setCols((prev) => [...prev, { name: `col_${prev.length + 1}`, type: "text", nullable: true }]);
  }
  function removeCol(i: number) {
    setCols((prev) => prev.filter((_, ix) => ix !== i));
  }
  function update(i: number, patch: Partial<Column>) {
    setCols((prev) => prev.map((c, ix) => (ix === i ? { ...c, ...patch } : c)));
  }

  // Drag & drop reorder for columns
  const dragIdx = useRef<number | null>(null);
  function onDragStart(i: number, e: React.DragEvent) {
    dragIdx.current = i;
    e.dataTransfer.setData("text/plain", String(i));
    e.dataTransfer.effectAllowed = "move";
  }
  function onDragOver(e: React.DragEvent) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  }
  function onDrop(i: number, e: React.DragEvent) {
    e.preventDefault();
    const from = dragIdx.current ?? Number(e.dataTransfer.getData("text/plain"));
    if (isNaN(from) || from === i) return;
    setCols((prev) => {
      const next = prev.slice();
      const [moved] = next.splice(from, 1);
      next.splice(i, 0, moved);
      return next;
    });
    dragIdx.current = null;
  }

  async function copyDDL() {
    try {
      await navigator.clipboard.writeText(ddl);
      flash("Copied DDL to clipboard");
    } catch {
      flash("Clipboard blocked");
    }
  }

  function doImport() {
    const sql = prompt("Paste CREATE TABLE ...; SQL");
    if (!sql) return;
    const parsed = importDDL(sql);
    if (!parsed) {
      alert("Could not parse that DDL");
      return;
    }
    setName(parsed.name);
    setCols(parsed.columns);
    flash("Imported from DDL");
  }

  return (
    <div className="grid gap-4">
      {/* Schema Selector (top) */}
      <Card className="bg-zinc-900/60 border-zinc-800">
        <CardHeader className="flex items-center justify-between">
          <CardTitle>Schema Selector</CardTitle>
          <div className="flex items-center gap-2">
            <Button variant="outline" className="border-zinc-700" onClick={addSchema}>
              <Upload className="w-4 h-4" /> New schema
            </Button>
            <Button variant="outline" className="border-zinc-700" onClick={saveSchema}>
              <Copy className="w-4 h-4" /> Save schema
            </Button>
          </div>
        </CardHeader>
        <CardContent className="flex flex-wrap items-start gap-2">
          {schemas.map((s, i) => (
            <button
              key={i}
              onClick={() => setActiveIdx(i)}
              className={`px-3 py-2 rounded-xl border text-sm inline-flex items-center gap-2 ${
                i === activeIdx
                  ? "border-emerald-600 bg-emerald-900/20"
                  : "border-zinc-800 hover:bg-zinc-800/50"
              }`}
              title={s.name}
            >
              {s.name}
            </button>
          ))}
          <div className="ml-auto">
            <Button
              variant="outline"
              className="border-red-700 text-red-300 hover:bg-red-900/20"
              onClick={() => deleteSchema(activeIdx)}
              disabled={schemas.length === 1}
              title="Delete current schema"
            >
              <Trash2 className="w-4 h-4" /> Delete
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* Builder + DDL (top-aligned columns) */}
      <Card className="bg-zinc-900/60 border-zinc-800">
        <CardHeader className="flex items-center justify-between">
          <CardTitle>Table Builder</CardTitle>
          <div className="flex items-center gap-2">
            <Button variant="outline" className="border-zinc-700" onClick={doImport} title="Import from CREATE TABLE DDL">
              <Upload className="w-4 h-4" /> Import DDL
            </Button>
            <Button variant="outline" className="border-zinc-700" onClick={copyDDL} title="Copy DDL">
              <Copy className="w-4 h-4" /> Copy DDL
            </Button>
            <Button onClick={addCol}>
              <Plus className="w-4 h-4" /> Add column
            </Button>
          </div>
        </CardHeader>

        {/* Top alignment on both sides */}
        <CardContent className="grid md:grid-cols-2 gap-4 items-start">
          {/* LEFT: table name + columns */}
          <div className="grid gap-3 self-start">
            <label className="text-sm">Table name</label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="bg-zinc-950 border-zinc-800"
            />

            <div className="grid gap-2 mt-2">
              <div className="grid grid-cols-[24px_1fr_1fr_110px_1fr_36px] gap-2 text-xs text-zinc-400 px-1">
                <div></div>
                <div>Column</div>
                <div>Type</div>
                <div>Nullable</div>
                <div>Default</div>
                <div></div>
              </div>

              {cols.map((c, i) => (
                <div
                  key={i}
                  className="grid grid-cols-[24px_1fr_1fr_110px_1fr_36px] gap-2 items-center bg-zinc-950/60 border border-zinc-800 rounded-xl px-2 py-2"
                  draggable
                  onDragStart={(e) => onDragStart(i, e)}
                  onDragOver={onDragOver}
                  onDrop={(e) => onDrop(i, e)}
                  title="Drag to reorder"
                >
                  <div className="flex items-center justify-center cursor-grab active:cursor-grabbing select-none">
                    <GripVertical className="w-4 h-4 opacity-70" />
                  </div>

                  <Input
                    value={c.name}
                    onChange={(e) => update(i, { name: e.target.value })}
                    className="bg-zinc-950 border-zinc-800"
                    placeholder="column_name"
                  />

                  <select
                    value={c.type}
                    onChange={(e) => update(i, { type: e.target.value })}
                    className="bg-zinc-950 border border-zinc-800 rounded-xl px-3 py-2 text-sm"
                  >
                    {TYPE_OPTS.map((t) => (
                      <option key={t} value={t}>
                        {t}
                      </option>
                    ))}
                  </select>

                  <label className="flex items-center justify-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={!c.nullable}
                      onChange={(e) => update(i, { nullable: !e.target.checked })}
                    />
                    <span className="hidden md:inline">NOT&nbsp;NULL</span>
                  </label>

                  <Input
                    value={c.default ?? ""}
                    onChange={(e) => update(i, { default: e.target.value || undefined })}
                    className="bg-zinc-950 border-zinc-800"
                    placeholder={STRINGY.test(c.type) ? "e.g. admin" : "e.g. 0 or now()"}
                  />

                  <button
                    className="inline-flex items-center justify-center w-8 h-8 rounded-lg hover:bg-zinc-800/60"
                    onClick={() => removeCol(i)}
                    title="Remove column"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              ))}
            </div>

            {msg && <div className="text-xs text-emerald-400 mt-1">{msg}</div>}
          </div>

          {/* RIGHT: DDL preview */}
          <div className="grid gap-2 self-start">
            <label className="text-sm">DDL Preview</label>
            <Textarea
              value={ddl}
              readOnly
              className="bg-zinc-950 border-zinc-800 font-mono h-[340px]"
            />
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

const STRINGY = /char|text|uuid|json|citext|inet|macaddr/i;
