export type User = { id: string; email: string; name?: string } | null;

export interface AuthClient {
  getUser(): Promise<User> | User;
  isAuthenticated(): Promise<boolean> | boolean;
  login(email: string, password: string): Promise<User>;
  logout(): Promise<void>;
  onChange?(cb: (u: User) => void): () => void; // unsubscribe
}

/** Lightweight event emitter for auth changes */
class Emitter<T> {
  private subs = new Set<(v: T) => void>();
  emit(v: T) { this.subs.forEach((f) => f(v)); }
  sub(fn: (v: T) => void) { this.subs.add(fn); return () => this.subs.delete(fn); }
}

/** Storage-backed helpers */
const KEY = "mt.session.v1";
const read = () => {
  try { return JSON.parse(localStorage.getItem(KEY) || "null") as User; } catch { return null; }
};
const write = (u: User) => localStorage.setItem(KEY, JSON.stringify(u));
const clear = () => localStorage.removeItem(KEY);

/** Mock auth for development */
export class MockAuth implements AuthClient {
  private emitter = new Emitter<User>();
  private user: User = read();

  getUser(): User { return this.user; }
  isAuthenticated(): boolean { return !!this.user; }

  async login(email: string, password: string): Promise<User> {
    // Trivial mock: accept a couple of dev creds, otherwise any non-empty pair
    if (!email || !password) throw new Error("Missing credentials");
    const allowed = [
      { email: "admin@example.com", password: "admin123", name: "Admin" },
      { email: "dev@example.com", password: "dev123", name: "Developer" },
    ];
    const match = allowed.find((x) => x.email === email && x.password === password);
    const user: User = { id: crypto.randomUUID(), email, name: match?.name ?? email.split("@")[0] };
    this.user = user; write(user); this.emitter.emit(user);
    return user;
  }

  async logout(): Promise<void> { this.user = null; clear(); this.emitter.emit(this.user); }

  onChange(cb: (u: User) => void) { return this.emitter.sub(cb); }
}

/** Placeholder for a future real client (e.g., OIDC, custom backend). */
export class TokenAuth implements AuthClient {
  private emitter = new Emitter<User>();
  private user: User = read();
  constructor(private readonly verifyToken: (token: string) => Promise<User | null>) {}
  getUser(): User { return this.user; }
  isAuthenticated(): boolean { return !!this.user; }
  async login(email: string, password: string): Promise<User> {
    // Replace with real call; for now we still mock a token
    const token = btoa(`${email}:${password}`);
    const user = (await this.verifyToken(token)) ?? { id: crypto.randomUUID(), email };
    write(user); this.user = user; this.emitter.emit(user); return user;
  }
  async logout(): Promise<void> { this.user = null; clear(); this.emitter.emit(this.user); }
  onChange(cb: (u: User) => void) { return this.emitter.sub(cb); }
}

/** Factory toggled by env (default mock) */
export function makeAuthClient(): AuthClient {
  const mode = import.meta.env.VITE_AUTH_MODE?.toLowerCase();
  if (mode === "token") {
    return new TokenAuth(async (_token) => {
      // TODO: call backend to validate token and return user
      return null; // cause fallback user above
    });
  }
  return new MockAuth();
}
