export type Column = {
  name: string;
  type: string;
  nullable: boolean;
  default?: string;
};

export type TableDef = {
  name: string;
  columns: Column[];
};

export type Schema = {
  name: string;
  tables: TableDef[];
};

export type RaftNode = {
  id: string;
  address: string;
  role: "leader" | "follower" | "candidate";
  term: number;
  commitIndex: number;
  matchedIndex: number;
  healthy: boolean;
  lagMs: number;
};

export type LogLevel = "DEBUG" | "INFO" | "WARN" | "ERROR";

export type LogRow = {
  time: string;   // ISO timestamp
  level: LogLevel;
  msg: string;
  ctx: string;    // component/context (e.g., "raft", "db", "auth")
  source: string; // e.g., src/raft/leader.ts
  fn: string;     // e.g., appendEntries
  line: number;   // 1-based line number
};