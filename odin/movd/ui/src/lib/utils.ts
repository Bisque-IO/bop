import type { TableDef, Column } from "@/lib/types";
import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

const STRINGY = /char|text|uuid|json|citext|inet|macaddr/i;

export function renderDDL(table: TableDef): string {
  const cols = table.columns.map(c => {
    const type = c.type.trim();
    const nn = c.nullable ? "" : " NOT NULL";
    const def = c.default?.trim();
    const needsQuotes = !!def && STRINGY.test(type) && !/^\w+\(.*\)$/.test(def) && !/^(true|false|null|\d+(\.\d+)?)$/i.test(def);
    const defSql = def ? ` DEFAULT ${needsQuotes ? `'${def.replaceAll("'", "''")}'` : def}` : "";
    return `  ${sanitizeIdent(c.name)} ${type}${nn}${defSql}`;
  }).join(",\n");
  return `CREATE TABLE ${sanitizeIdent(table.name)} (\n${cols}\n);`;
}

export function sanitizeIdent(name: string): string {
  // keep it simple: letters, digits, underscore; otherwise quote
  if (/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) return name;
  const q = name.replaceAll(`"`, `""`);
  return `"${q}"`;
}

// --------- basic DDL importer (best‑effort) ----------
export function importDDL(sql: string): TableDef | null {
  const m = /create\s+table\s+([^(]+)\s*\(([\s\S]+)\)\s*;/i.exec(sql);
  if (!m) return null;
  const tableName = m[1].trim().replace(/^"|"$/g, "");
  const body = m[2];
  const parts = splitTopLevel(body);
  const columns: Column[] = [];
  for (const raw of parts) {
    const line = raw.trim().replace(/,+$/, "");
    if (!line) continue;
    // skip constraints for now
    if (/^(primary|unique|foreign|check|constraint)\b/i.test(line)) continue;
    const nmType = /^("?[^"\s]+"?|\w+)\s+(.+)$/.exec(line);
    if (!nmType) continue;
    const name = nmType[1].replace(/^"|"$/g, "");
    let rest = nmType[2].trim();

    // extract DEFAULT
    let _default: string | undefined;
    const defM = /\bdefault\b\s+(.+?)(?:\s+not\s+null|\s+null|\s*,|$)/i.exec(rest);
    if (defM) {
      _default = defM[1].trim().replace(/^'(.*)'$/s, "$1");
      rest = rest.replace(defM[0], "").trim();
    }
    // nullable
    const nullable = !/\bnot\s+null\b/i.test(rest);

    // remaining first token(s) is type
    const type = rest.replace(/\bnot\s+null\b/ig, "").replace(/\bnull\b/ig, "").replace(/\bdefault\b[\s\S]*$/i, "").trim().replace(/,+$/, "");
    columns.push({ name, type: type || "text", nullable, default: _default });
  }
  return { name: tableName, columns };
}

function splitTopLevel(s: string): string[] {
  const out: string[] = [];
  let buf = "", depth = 0, inStr: string | null = null;
  for (let i=0;i<s.length;i++) {
    const ch = s[i], prev = s[i-1];
    if (inStr) {
      buf += ch;
      if (ch === inStr && prev !== "\\") inStr = null;
      continue;
    }
    if (ch === "'" || ch === `"`) { inStr = ch; buf += ch; continue; }
    if (ch === "(") { depth++; buf += ch; continue; }
    if (ch === ")") { depth = Math.max(0, depth-1); buf += ch; continue; }
    if (ch === "," && depth === 0) { out.push(buf); buf = ""; continue; }
    buf += ch;
  }
  if (buf.trim()) out.push(buf);
  return out;
}
