import * as React from "react"
import { Card } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select"
import { Badge } from "@/components/ui/badge"
import { Crown, Shuffle, RotateCcw, SlidersHorizontal } from "lucide-react"

type Node = { id: number; label: string; ip: string }
type LinkState = "synced" | "syncing" | "broken" | "offline"
const STATE_ORDER: LinkState[] = ["offline", "syncing", "synced", "broken"]

export default function RaftTopology() {
  const [count, setCount] = React.useState(5)
  const [leader, setLeader] = React.useState(0)
  const [showArrows, setShowArrows] = React.useState(false)
  const [preset, setPreset] = React.useState<"All Synced" | "Mixed">("All Synced")

  // nodes + fake IPs
  const nodes = React.useMemo<Node[]>(
    () =>
      Array.from({ length: count }, (_, i) => ({
        id: i,
        label: `n${i + 1}`,
        ip: `10.0.0.${i + 11}:50${String(i + 1).padStart(2, "0")}`,
      })),
    [count]
  )

  // link state from leader -> follower
  const [linkState, setLinkState] = React.useState<Record<number, LinkState>>({})
  React.useEffect(() => {
    const next: Record<number, LinkState> = {}
    for (let i = 0; i < nodes.length; i++) {
      if (i === leader) continue
      next[i] = preset === "All Synced" ? "synced" : randomState()
    }
    setLinkState(next)
  }, [nodes.length, leader, preset])

  const size = 380
  const cx = size / 2
  const cy = size / 2
  const radius = 135

  const coords = (i: number) => {
    const angle = (i / nodes.length) * Math.PI * 2 - Math.PI / 2
    return { x: cx + radius * Math.cos(angle), y: cy + radius * Math.sin(angle) }
  }

  const onCycle = (i: number) => {
    setLinkState((prev) => {
      const current = prev[i] ?? "synced"
      const idx = (STATE_ORDER.indexOf(current) + 1) % STATE_ORDER.length
      return { ...prev, [i]: STATE_ORDER[idx] }
    })
  }

  return (
    <Card className="bg-zinc-900/60 border-zinc-800 backdrop-blur supports-[backdrop-filter]:bg-zinc-900/50 p-4">
      <style>{`
        @keyframes dashMove { to { stroke-dashoffset: -20; } }
        @keyframes blinkSoft { 0%,100% { opacity: .35 } 50% { opacity: 1 } }
        @keyframes pulseGlow {
          0%,100% { filter: drop-shadow(0 0 0 rgba(16,185,129,.0)); }
          50% { filter: drop-shadow(0 0 6px rgba(16,185,129,.35)); }
        }
      `}</style>

      <div className="flex items-center justify-between gap-3 mb-3">
        <div className="flex items-center gap-2">
          <Badge variant="outline" className="border-ghost text-zinc-300">Raft Topology</Badge>
          <div className="text-zinc-400 text-sm">Leader → follower replication states</div>
        </div>

        <div className="flex items-center gap-2">
          <Select
            value={String(count)}
            onValueChange={(v) => {
              const n = Number(v)
              setCount(n)
              setLeader((prev) => prev % n)
            }}
          >
            <SelectTrigger className="w-[132px]">
              <SlidersHorizontal className="mr-2 h-4 w-4" />
              <SelectValue placeholder="ALL" />
            </SelectTrigger>
            <SelectContent>
              {[3, 4, 5, 6, 7].map((n) => (
                <SelectItem key={n} value={String(n)}>{n} nodes</SelectItem>
              ))}
            </SelectContent>
          </Select>

          <Select value={preset} onValueChange={(v: any) => setPreset(v)}>
            <SelectTrigger className="w-[160px]">
              <Shuffle className="mr-2 h-4 w-4" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="All Synced">All Synced</SelectItem>
              <SelectItem value="Mixed">Mixed</SelectItem>
            </SelectContent>
          </Select>

          <Button variant="outline" onClick={() => setLeader((leader + 1) % nodes.length)}>
            <Crown className="mr-2 h-4 w-4" />
            Rotate leader
          </Button>

          <Button variant="ghost" onClick={() => { setPreset("All Synced"); setLeader(0) }}>
            <RotateCcw className="mr-2 h-4 w-4" />
            Reset
          </Button>
        </div>
      </div>

      <div className="relative mx-auto pt-5">
        <svg width={size} height={size} className="block mx-auto">
          {/* LINKS: render first so nodes sit above them */}
          {nodes.map((n, i) => {
            if (i === leader) return null
            const from = coords(leader)
            const to = coords(i)
            const st = linkState[i] ?? "synced"
            const style = styleFor(st)
            const mid = { x: (from.x + to.x) / 2, y: (from.y + to.y) / 2 }

            return (
              <g key={`link-${leader}-${i}`} className="cursor-pointer" onClick={() => onCycle(i)}>
                <line
                  x1={from.x} y1={from.y} x2={to.x} y2={to.y}
                  stroke={style.stroke}
                  strokeWidth={style.width}
                  strokeDasharray={style.dasharray}
                  style={style.inline}
                />
                {/* state chip */}
                <rect
                  x={mid.x - 31} y={mid.y - 10} rx={6} ry={6} width={61} height={20}
                  className="fill-zinc-900 stroke-zinc-500"
                  strokeWidth={0.75}
                  style={{ stroke: style.stroke }}
                />
                <text
                  x={mid.x} y={mid.y}
                  textAnchor="middle"
                  alignmentBaseline="middle"
                  className="text-[10px] fill-zinc-300"
                >
                  {st}
                </text>
              </g>
            )
          })}

          {/* NODES */}
          {nodes.map((n, i) => {
            const { x, y } = coords(i)
            const isLeader = i === leader

            // Measure text widths to size circle & chip
            const { width, height } = measureNodeBounds(n.label, n.ip)
            const circleDiameter = Math.max(width + 10 /* ≥5px padding each side */, height + 12 /* top/btm breathing */)
            const r = Math.max(24, Math.ceil(circleDiameter / 2))

            const st = linkState[i] ?? "synced"
            const style = styleFor(st)

            return (
              <g key={n.id} transform={`translate(${x}, ${y})`} className="cursor-pointer" onClick={() => setLeader(i)}>
                {/* 1) Circle */}
                <circle
                  r={r}
                  className={isLeader
                    ? "fill-emerald-900 stroke-emerald-400 stroke-2"
                    : "fill-zinc-900 stroke-zinc-700 stroke-[1.5]"
                  }
                  style={isLeader ? { animation: "pulseGlow 2.2s ease-in-out infinite" } : style}
                />

                {/* 2) Texts on TOP */}
                <text
                  x={0}
                  y={-5}
                  textAnchor="middle"
                  alignmentBaseline="baseline"
                  className="text-[12px] fill-zinc-200"
                  style={{ pointerEvents: "none" }}
                >
                  {n.label}
                </text>
                <text
                  x={0}
                  y={3}
                  textAnchor="middle"
                  alignmentBaseline="hanging"
                  className="text-[10px] fill-zinc-400"
                  style={{ pointerEvents: "none" }}
                >
                  {n.ip}
                </text>
              </g>
            )
          })}

        </svg>

        {/* LEGEND */}
        <div className="mt-3 flex flex-wrap items-center justify-center gap-4 text-xs text-zinc-400">
          <LegendSwatch color="#34d399" label="synced" />
          <LegendSwatch color="#fbbf24" dashed label="Catching‑Up" />
          <LegendSwatch color="#ef4444" blinking label="broken" />
          <LegendSwatch color="#9ca3af" dotted label="offline" />
          <div className="flex items-center gap-2">
            <span className="inline-block h-3 w-3 rounded-full bg-emerald-500/40 ring-1 ring-emerald-400" />
            Leader node
          </div>
          <div className="text-zinc-500">(Click a link to cycle state)</div>
        </div>
      </div>
    </Card>
  )
}

/** Legend chip */
function LegendSwatch({
  color, label, dashed, dotted, blinking,
}: { color: string; label: string; dashed?: boolean; dotted?: boolean; blinking?: boolean }) {
  return (
    <div className="flex items-center gap-2">
      <svg width="24" height="8">
        <line
          x1="1" y1="4" x2="23" y2="4"
          stroke={color}
          strokeWidth={2}
          strokeDasharray={dashed ? "6 6" : dotted ? "2 6" : undefined}
          style={blinking ? { animation: "blinkSoft 1.4s ease-in-out infinite" } : undefined}
        />
      </svg>
      <span>{label}</span>
    </div>
  )
}

/** Link styles */
function styleFor(state: LinkState) {
  switch (state) {
    case "synced":
      return { stroke: "#34d399", width: 2, dasharray: undefined as string | undefined, inline: { animation: "none" as const, opacity: 1 }, showArrow: true }
    case "syncing":
      return { stroke: "#fbbf24", width: 2, dasharray: "6 6", inline: { animation: "dashMove 1.2s linear infinite", opacity: 0.95 }, showArrow: true }
    case "broken":
      return { stroke: "#ef4444", width: 2.25, dasharray: "10 6", inline: { animation: "blinkSoft 1.2s ease-in-out infinite", opacity: 1 }, showArrow: true }
    case "offline":
      return { stroke: "#9ca3af", width: 1.75, dasharray: "2 6", inline: { animation: "none" as const, opacity: 0.45 }, showArrow: false }
  }
}

/** Accurate text measurement with an offscreen canvas */
const _measureCanvas = typeof document !== "undefined" ? document.createElement("canvas") : null
const _ctx = _measureCanvas ? _measureCanvas.getContext("2d") : null
function measureNodeBounds(name: string, ip: string) {
  // Fallback approximations if canvas isn't available (SSR)
  if (!_ctx) {
    const approx = (s: string, pxPerChar: number) => s.length * pxPerChar
    const w = Math.max(approx(name, 7), approx(ip, 6))
    const h = 12 + 10 + 6 // font12 + font10 + spacing
    return { width: w, height: h }
  }
  // Use the typical UI font stacks; sizes match <text> elements above
  let nameWidth = 0
  _ctx.font = "12px ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial"
  nameWidth = _ctx.measureText(name).width
  _ctx.font = "10px ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial"
  const ipWidth = _ctx.measureText(ip).width

  const width = Math.max(nameWidth, ipWidth)
  const height = 12 + 10 + 6 // 6px line gap
  return { width, height }
}

function randomState(): LinkState {
  const pool: LinkState[] = ["synced", "syncing", "broken", "offline"]
  return pool[Math.floor(Math.random() * pool.length)]
}

function arrowHead(x1: number, y1: number, x2: number, y2: number, len = 8, spread = 5) {
  const angle = Math.atan2(y2 - y1, x2 - x1)
  const ax = x2 - len * Math.cos(angle)
  const ay = y2 - len * Math.sin(angle)
  const left = `${ax + spread * Math.sin(angle)},${ay - spread * Math.cos(angle)}`
  const tip = `${x2},${y2}`
  const right = `${ax - spread * Math.sin(angle)},${ay + spread * Math.cos(angle)}`
  return `${left} ${tip} ${right}`
}

function CrownSVG(props: React.SVGProps<SVGSVGElement>) {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" {...props}>
      <path d="M3 20h18v-2l-3-9-6 5-6-5-3 9v2z" />
    </svg>
  )
}
