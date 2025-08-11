import React, { createContext, useContext, useEffect, useMemo, useState } from "react";
import { AuthClient, User, makeAuthClient } from "./auth";

interface AuthState {
  user: User;
  loading: boolean;
  login: (email: string, password: string) => Promise<void>;
  logout: () => Promise<void>;
}

const Ctx = createContext<AuthState | null>(null);

export function useAuth() {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useAuth must be used within <AuthProvider>");
  return ctx;
}

export function AuthProvider({ children, client }: { children: React.ReactNode; client?: AuthClient }) {
  const auth = useMemo(() => client ?? makeAuthClient(), [client]);
  const [user, setUser] = useState<User>(() => (typeof window !== "undefined" ? auth.getUser() : null));
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    return auth.onChange?.(setUser);
  }, [auth]);

  const value: AuthState = {
    user,
    loading,
    login: async (email, password) => {
      setLoading(true);
      try { await auth.login(email, password); }
      finally { setLoading(false); }
    },
    logout: async () => { await auth.logout(); },
  };

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
