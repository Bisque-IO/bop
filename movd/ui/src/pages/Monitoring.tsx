import React from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useMockMetrics } from "@/lib/mock";
import {
  LineChart,
  Line,
  CartesianGrid,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
} from "recharts";

const METRICS = [
  { key: "qps", label: "QPS" },
  { key: "latency", label: "Latency (ms)" },
  { key: "cpu", label: "CPU (%)" },
];

export default function Monitoring() {
  const data = useMockMetrics();

  return (
    <div className="grid lg:grid-cols-2 gap-4">
      {METRICS.map((m, i) => (
        <Card key={m.key} className="bg-zinc-900/60 border-zinc-800">
          <CardHeader>
            <CardTitle>{m.label}</CardTitle>
          </CardHeader>
          {/* Use a fixed height so ResponsiveContainer can measure */}
          <CardContent className="h-56">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={data} margin={{ top: 8, right: 12, left: 0, bottom: 4 }}>
                <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,0.08)" />
                <XAxis
                  dataKey="t"
                  stroke="rgba(255,255,255,0.6)"
                  tickLine={false}
                  axisLine={false}
                />
                <YAxis
                  stroke="rgba(255,255,255,0.6)"
                  tickLine={false}
                  axisLine={false}
                />
                <Tooltip
                  contentStyle={{
                    background: "rgba(24,24,27,0.9)",
                    border: "1px solid #27272a",
                  }}
                  labelStyle={{ color: "rgba(255,255,255,0.8)" }}
                />
                <Line
                  type="monotone"
                  dataKey={m.key}
                  dot={false}
                  strokeWidth={2}
                  isAnimationActive={false}
                  stroke="rgba(99,102,241,0.95)" // readable on dark glass
                  activeDot={{ r: 3 }}
                />
              </LineChart>
            </ResponsiveContainer>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}
