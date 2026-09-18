export type SyncStatus = "idle" | "running" | "success" | "failed" | "stopped";

export interface Repo {
  id: string;
  name: string;
  source: string;
  target: string;
  lastSynced: number | null;
  lastStatus: SyncStatus;
  lastMessage: string | null;
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
  port: number;
  apiKey: string;
}

export interface OrgInfo {
  login: string;
  name: string;
  avatarUrl: string;
  description: string;
}

export interface AccountInfo {
  platform: string;
  login: string;
  name: string;
  avatarUrl: string;
  orgs: OrgInfo[];
}

export type ProviderPlatform = "github" | "gitlab";

export interface ProviderInfo {
  platform: string;
  hasPat: boolean;
  patMasked: string | null;
}

export interface SyncEvent {
  id: string;
  status: SyncStatus;
  message: string | null;
  lastSynced: number | null;
}
