import { describe, it, expect } from "vitest";
import { generateLogs } from "@/lib/mock";
import { renderDDL } from "@/lib/utils";

describe("lib helpers", () => {
  it("generateLogs returns 1000 rows with valid levels", () => {
    const rows = generateLogs();
    expect(rows.length).toBe(1000);
    const levels = new Set(rows.map(r => r.level));
    for (const lv of levels) expect(["DEBUG","INFO","WARN","ERROR"]).toContain(lv);
  });
  it("renderDDL contains table and columns", () => {
    const ddl = renderDDL({ name: "t", columns: [{ name: "id", type: "uuid", nullable: false }] });
    expect(ddl).toMatch(/CREATE TABLE t/);
    expect(ddl).toMatch(/id uuid/);
  });
});
