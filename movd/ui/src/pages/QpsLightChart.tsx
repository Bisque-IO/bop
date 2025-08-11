import React, { useEffect, useMemo, useRef } from "react";
import { createChart, LineSeries, ColorType, type IChartApi, type ISeriesApi } from "lightweight-charts";

type Pt = { t: number; qps: number };

const BG = "#121214";      // pure black
const GRID = "#222222";    // muted grid like your ref
const TEXT = "#a1a1aa";
const LINE = "#6b6bff";    // bluish‑purple stroke

export default function QpsLightChart({ data }: { data: Pt[] }) {
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const chartRef = useRef<IChartApi | null>(null);
  const seriesRef = useRef<ISeriesApi<"Line"> | null>(null);

  // stable integer timestamps so the scale doesn’t bunch
  const STEP = 2;
  const t0 = useRef<number | null>(null);
  if (t0.current == null) {
    const now = Math.floor(Date.now() / 1000);
    t0.current = now - (data.length - 1) * STEP;
  }
  const points = useMemo(
    () => data.map((d, i) => ({ time: t0.current! + i * STEP, value: d.qps })),
    [data]
  );

  useEffect(() => {
    const el = wrapRef.current;
    if (!el) return;

    // solid black container so the card’s translucent bg can’t bleed through
    el.style.background = BG;

    const chart = createChart(el, {
      autoSize: true,
      layout: { background: { type: ColorType.Solid, color: BG }, textColor: TEXT, attributionLogo: false },
      grid: { vertLines: { color: GRID }, horzLines: { color: GRID } },
      rightPriceScale: { borderColor: GRID },
      timeScale: { borderColor: GRID, fixLeftEdge: false, fixRightEdge: false, timeVisible: true, secondsVisible: true },
    });
    chartRef.current = chart;

    const series = chart.addSeries(LineSeries, {
      color: LINE,
      lineWidth: 2,
      priceLineVisible: true,
      lastValueVisible: true,
      crosshairMarkerVisible: true,
    });
    seriesRef.current = series;

    if (points.length) {
      series.setData(points);
      chart.timeScale().fitContent();
    }

    const ro = new ResizeObserver(() => chart.timeScale().fitContent());
    ro.observe(el);

    return () => {
      ro.disconnect();
      chart.remove();
    };
  }, []); // mount once

  useEffect(() => {
    if (!seriesRef.current || !chartRef.current) return;
    seriesRef.current.setData(points);
    // chartRef.current.timeScale().fitContent();
  }, [points]);

  return (
    <div
      ref={wrapRef}
      className="w-full h-full overflow-hidden rounded-xl" // stays inside card body
    />
  );
}
