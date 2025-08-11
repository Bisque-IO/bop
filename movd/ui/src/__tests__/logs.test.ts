import { describe, it, expect } from "vitest";
import { computeTemplate, reorder, DEFAULT_WIDTHS, type ColKey } from "@/pages/Logs/logic";

describe("logs column logic (pure functions)", () => {
  const order: ColKey[] = ["time","level","msg","ctx"];
  it("computeTemplate returns css grid template", () => {
    const tpl = computeTemplate(order, DEFAULT_WIDTHS);
    expect(tpl.split(" ").length).toBe(4);
    expect(tpl).toMatch(/px/);
  });
  it("reorder moves a column correctly", () => {
    const next = reorder(order, "msg", "time");
    expect(next).toEqual(["msg","time","level","ctx"]);
  });
  it("reorder is stable when keys are invalid", () => {
    const next = reorder(order, "msg", "msg");
    expect(next).toEqual(order);
  });
});
