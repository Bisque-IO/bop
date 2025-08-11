import React, { useState } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { useAuth } from "../auth/AuthProvider";
import { Card, CardContent, CardHeader, CardTitle } from "../components/ui/card";
import { Label } from "../components/ui/label";
import { Input } from "../components/ui/input";
import { Button } from "../components/ui/button";
import { Loader2, LogIn } from "lucide-react";

export default function Login() {
  const { login, loading } = useAuth();
  const nav = useNavigate();
  const loc = useLocation() as any;
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      await login(email, password);
      const to = loc.state?.from?.pathname ?? "/";
      nav(to, { replace: true });
    } catch (err: any) {
      setError(err?.message ?? "Login failed");
    }
  }

  return (
    <div className="min-h-dvh grid place-items-center bg-gradient-to-b from-background to-background/60">
      <Card className="w-full max-w-md bg-zinc-900/60 border-zinc-800 backdrop-blur supports-[backdrop-filter]:bg-zinc-900/50">
        <CardHeader>
          <CardTitle className="text-center">Sign in</CardTitle>
        </CardHeader>
        <CardContent>
          <form onSubmit={onSubmit} className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="email">Email</Label>
              <Input id="email" type="email" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="you@example.com" required />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="password">Password</Label>
              <Input id="password" type="password" value={password} onChange={(e) => setPassword(e.target.value)} required />
            </div>
            {error && (
              <div className="text-sm text-red-400" role="alert">{error}</div>
            )}
            <Button type="submit" disabled={loading} className="mt-2">
              {loading ? <Loader2 className="mr-2 h-4 w-4 animate-spin" /> : <LogIn className="mr-2 h-4 w-4" />}
              <span>Log in</span>
            </Button>
            <p className="text-xs text-zinc-400 mt-2">
              Hint (mock): admin@example.com / admin123
            </p>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
