import React from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

export default function Settings(){
  return (
    <Card className="bg-zinc-900/60 border-zinc-800 max-w-xl">
      <CardHeader><CardTitle>Settings</CardTitle></CardHeader>
      <CardContent className="grid gap-3">
        <label className="text-sm">API base URL</label>
        <Input placeholder="https://api.example.com" className="bg-zinc-950 border-zinc-800" />
        <Button className="mt-2">Save</Button>
      </CardContent>
    </Card>
  );
}
