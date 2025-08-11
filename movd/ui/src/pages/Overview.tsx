import React, { useEffect, useMemo, useRef } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useMockMetrics } from "@/lib/mock";
import {
  LineChart as RCLineChart,
  Line,
  CartesianGrid,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import { createChart, ColorType, IChartApi, LineStyle } from "lightweight-charts";

import QpsLightChart from "./QpsLightChart"; // Assuming this is the file where QpsLightChart is defined

export default function Overview() {
  const data = useMockMetrics(); // provided in src/lib/mock.ts:contentReference[oaicite:1]{index=1}

  const axisColor = "rgba(255,255,255,0.6)";
  const gridColor = "rgba(255,255,255,0.08)";
  const lineColor = "rgba(99,102,241,0.95)";

  return (
    <div className="grid gap-4">
      {/* QPS via lightweight-charts */}
      <Card className="bg-zinc-900/60 border-zinc-800">
        <CardHeader>
          <CardTitle>QPS</CardTitle>
        </CardHeader>
        <CardContent className="h-64 p-0">
          <QpsLightChart data={data} />
        </CardContent>
      </Card>

      {/* Latency & CPU stay on Recharts for now */}
      <div className="grid md:grid-cols-2 gap-4">
        <Card className="bg-zinc-900/60 border-zinc-800">
          <CardHeader><CardTitle>Latency (ms)</CardTitle></CardHeader>
          <CardContent className="h-56">
            <ResponsiveContainer width="100%" height="100%">
              <RCLineChart data={data} margin={{ top: 8, right: 12, left: 0, bottom: 4 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={gridColor} />
                <XAxis dataKey="t" stroke={axisColor} tickLine={false} axisLine={false} />
                <YAxis stroke={axisColor} tickLine={false} axisLine={false} />
                <Tooltip
                  contentStyle={{ background: "rgba(24,24,27,0.9)", border: "1px solid #27272a" }}
                  labelStyle={{ color: "rgba(255,255,255,0.8)" }}
                />
                <Line type="monotone" dataKey="latency" dot={false} strokeWidth={2} stroke={lineColor} isAnimationActive={false} />
              </RCLineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>

        <Card className="bg-zinc-900/60 border-zinc-800">
          <CardHeader><CardTitle>CPU (%)</CardTitle></CardHeader>
          <CardContent className="h-56">
            <ResponsiveContainer width="100%" height="100%">
              <RCLineChart data={data} margin={{ top: 8, right: 12, left: 0, bottom: 4 }}>
                <CartesianGrid strokeDasharray="3 3" stroke={gridColor} />
                <XAxis dataKey="t" stroke={axisColor} tickLine={false} axisLine={false} />
                <YAxis stroke={axisColor} tickLine={false} axisLine={false} />
                <Tooltip
                  contentStyle={{ background: "rgba(24,24,27,0.9)", border: "1px solid #27272a" }}
                  labelStyle={{ color: "rgba(255,255,255,0.8)" }}
                />
                <Line type="monotone" dataKey="cpu" dot={false} strokeWidth={2} stroke={lineColor} isAnimationActive={false} />
              </RCLineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
