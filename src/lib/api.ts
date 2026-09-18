import { invoke } from "@tauri-apps/api/core";
import type {
  AccountInfo,
  AppInfo,
  AvailableRepo,
  LoginResult,
  McpConfig,
  OperationLog,
  ProviderInfo,
  Repo,
} from "./types";

export const api = {
  login(username: string, password: string, remember: boolean) {
    return invoke<LoginResult>("login", { username, password, remember });
  },
  restoreSession(token: string) {
    return invoke<string>("restore_session", { token });
  },
  logout(token: string) {
    return invoke<void>("logout", { token });
  },
  changePassword(token: string, oldPassword: string, newPassword: string) {
    return invoke<void>("change_password", { token, oldPassword, newPassword });
  },
  listRepos(token: string) {
    return invoke<Repo[]>("list_repos", { token });
  },
  saveRepo(
    token: string,
    args: { id?: string | null; name: string; source: string; target: string },
  ) {
    return invoke<Repo>("save_repo", { token, ...args });
  },
  deleteRepo(token: string, id: string) {
    return invoke<void>("delete_repo", { token, id });
  },
  startSync(token: string, ids: string[]) {
    return invoke<number>("start_sync", { token, ids });
  },
  stopSync(token: string, id?: string | null) {
    return invoke<void>("stop_sync", { token, id: id ?? null });
  },
  getProviders(token: string) {
    return invoke<ProviderInfo[]>("get_providers", { token });
  },
  saveProvider(token: string, platform: string, pat: string) {
    return invoke<void>("save_provider", { token, platform, pat });
  },
  fetchAccounts(token: string, platform: string) {
    return invoke<AccountInfo>("fetch_provider_accounts", { token, platform });
  },
  getAppInfo(token: string) {
    return invoke<AppInfo>("get_app_info", { token });
  },
  getMcpConfig(token: string) {
    return invoke<McpConfig>("get_mcp_config", { token });
  },
  regenerateMcpKey(token: string) {
    return invoke<string>("regenerate_mcp_api_key", { token });
  },
  getBaseDir(token: string) {
    return invoke<string>("get_base_dir", { token });
  },
  setBaseDir(token: string, baseDir: string) {
    return invoke<void>("set_base_dir", { token, baseDir });
  },
  listLogs(token: string, limit?: number) {
    return invoke<OperationLog[]>("list_operation_logs", { token, limit: limit ?? null });
  },
  getPrimaryPlatform(token: string) {
    return invoke<string | null>("get_primary_platform", { token });
  },
  setPrimaryPlatform(token: string, platform: string) {
    return invoke<void>("set_primary_platform", { token, platform });
  },
  listAvailableRepos(token: string) {
    return invoke<AvailableRepo[]>("list_available_repos", { token });
  },
};
