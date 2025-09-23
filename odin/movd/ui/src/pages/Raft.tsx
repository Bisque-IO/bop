import React from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { useMockRaft } from "@/lib/mock";
import RaftTopology from "./RaftTopology";

export default function Raft(){
  const [nodes, setNodes] = useMockRaft();
  function transferLeadership(toId: string){
    setNodes(prev => prev.map(n => ({ ...n, role: n.id===toId ? "leader" : "follower" })));
  }
  return (
    <div className="space-y-4">
      <RaftTopology />
    <div className="grid md:grid-cols-2 lg:grid-cols-3 gap-4">
      {nodes.map(n => (
        <Card key={n.id} className={"bg-zinc-900/60 border-zinc-800 " + (n.role==="leader"?"ring-1 ring-emerald-600":"")}>
          <CardHeader className="flex items-center justify-between">
            <CardTitle className="text-base">{n.address}</CardTitle>
            <span className={"text-xs px-2 py-0.5 rounded-full " + (n.role==="leader"?"bg-emerald-900":"bg-zinc-800")}>{n.role}</span>
          </CardHeader>
          <CardContent className="text-sm grid gap-1">
            <div>term: {n.term}</div>
            <div>commitIndex: {n.commitIndex}</div>
            <div>matchedIndex: {n.matchedIndex}</div>
            <div>lag: {n.lagMs} ms</div>
            <div>healthy: {String(n.healthy)}</div>
            <div className="pt-2"><Button variant="outline" className="border-zinc-700" onClick={()=>transferLeadership(n.id)}>Make leader</Button></div>
          </CardContent>
        </Card>
      ))}
    </div>
    </div>
  );
}
