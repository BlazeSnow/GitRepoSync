export type PageKey = "sync" | "logs" | "mcp" | "settings";

export type SyncStatus = "idle" | "running" | "success" | "failed" | "stopped";

export interface TargetState {
  remote: string;
  url: string;
  lastStatus: SyncStatus;
  lastMessage: string | null;
  lastSynced: number | null;
}

export interface Repo {
  /** 仓库身份：基地址下的目录名 */
  name: string;
  source: string;
  lastSynced: number | null;
  lastStatus: SyncStatus;
  lastMessage: string | null;
  targets: TargetState[];
}

export interface LoginResult {
  token: string;
  username: string;
}

export interface AppInfo {
  name: string;
  version: string;
  os: string;
}

export interface McpConfig {
  apiKey: string;
  exePath: string;
}

export interface SyncEvent {
  name: string;
  status: SyncStatus;
  message: string | null;
  lastSynced: number | null;
}

export interface OperationLog {
  id: number;
  action: string;
  operator: string;
  createdAt: number;
}

