import { useEffect, useState, type Dispatch, type SetStateAction } from "react";
import type { LogRow, RaftNode } from "@/lib/types";

export const mkId = () => Math.random().toString(36).slice(2, 9);

export function useMockMetrics() {
  const [series, setSeries] = useState(
    Array.from({ length: 150 }).map((_, i) => ({
      t: new Date().getUTCSeconds(),
      qps: 400 + Math.round(Math.random()*150),
      latency: 10 + Math.round(Math.random()*10),
      cpu: 30 + Math.round(Math.random()*40)
    }))
  );
  useEffect(() => {
    const id = setInterval(() => {
      setSeries(prev => {
        const last = prev[prev.length-1]?.t ?? 0;
        return [...prev.slice(-149), {
          t: new Date().getUTCSeconds(),
          qps: 380 + Math.round(Math.random()*200),
          latency: 8 + Math.round(Math.random()*16),
          cpu: 25 + Math.round(Math.random()*55)
        }];
      });
    }, 1000);
    return () => clearInterval(id);
  }, []);
  return series;
}

export function useMockRaft(): [RaftNode[], Dispatch<SetStateAction<RaftNode[]>>] {
  const [nodes, setNodes] = useState<RaftNode[]>(() => {
    const leaderIdx = Math.floor(Math.random()*5);
    return Array.from({ length: 5 }).map((_, i) => ({
      id: mkId(),
      address: `10.0.0.${i+1}:7000`,
      role: i===leaderIdx ? "leader" : "follower",
      term: 42,
      commitIndex: 1000+i*10,
      matchedIndex: 1000+i*10-Math.floor(Math.random()*5),
      healthy: true,
      lagMs: Math.round(Math.random()*40)
    }));
  });
  useEffect(() => {
    const id = setInterval(() => {
      setNodes(prev => prev.map(n => ({
        ...n,
        commitIndex: n.commitIndex + Math.round(Math.random()*5),
        matchedIndex: n.matchedIndex + Math.round(Math.random()*5),
        lagMs: Math.max(0, Math.round(n.lagMs + (Math.random()*10 - 5))),
        healthy: Math.random() > 0.02
      })));
    }, 2000);
    return () => clearInterval(id);
  }, []);
  return [nodes, setNodes];
}

export function generateLogs(): LogRow[] {
  const levels = ["DEBUG","INFO","WARN","ERROR"] as const;
  const contexts = ["auth","db","raft","http","scheduler","billing"];
  const files = [
    "src/raft/leader.ts","src/raft/follower.ts","src/db/pool.ts",
    "src/http/server.ts","src/auth/jwt.ts","src/scheduler/timer.ts"
  ];
  const fns = ["appendEntries","requestVote","connect","query","handle","verify","tick"];
  const msgs = [
    "request completed","connected to peer","apply committed entry apply committed entry apply committed entry apply committed entry apply committed entry apply committed entry apply committed entry apply committed entry apply committed entry apply committed entry","retrying after error",
    "user login success","GC cycle finished","snapshot saved","schema migration started"
  ];

  const rows: LogRow[] = [];
  const now = Date.now();
  for (let i=0;i<1000;i++) {
    const ts = new Date(now - i*1000*(0.5+Math.random())).toISOString();
    rows.push({
      time: ts,
      level: levels[Math.floor(Math.random()*levels.length)],
      msg: msgs[Math.floor(Math.random()*msgs.length)],
      ctx: contexts[Math.floor(Math.random()*contexts.length)],
      source: files[Math.floor(Math.random()*files.length)],
      fn: fns[Math.floor(Math.random()*fns.length)],
      line: 10 + Math.floor(Math.random()*900)
    });
  }
  return rows;
}