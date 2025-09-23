import { describe, it, expect } from "vitest";
import { renderDDL, importDDL } from "@/lib/utils";

describe("DDL rendering and import", () => {
  it("quotes string defaults and preserves functions", () => {
    const sql = renderDDL({
      name: "t",
      columns: [
        { name: "id", type: "uuid", nullable: false },
        { name: "role", type: "text", nullable: false, default: "admin" },
        { name: "created_at", type: "timestamptz", nullable: false, default: "now()" },
        { name: "count", type: "int", nullable: true, default: "0" },
      ]
    });
    expect(sql).toMatch(/role text NOT NULL DEFAULT 'admin'/);
    expect(sql).toMatch(/created_at timestamptz NOT NULL DEFAULT now\(\)/);
    expect(sql).toMatch(/count int DEFAULT 0/);
  });

  it("imports a basic CREATE TABLE", () => {
    const ddl = `
      CREATE TABLE "users" (
        id uuid NOT NULL,
        name text NOT NULL DEFAULT 'alice',
        created_at timestamptz NOT NULL DEFAULT now()
      );
    `;
    const t = importDDL(ddl);
    expect(t?.name).toBe("users");
    expect(t?.columns.length).toBe(3);
    expect(t?.columns[1].default).toBe("alice");
  });
});
