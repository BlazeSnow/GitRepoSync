import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, LoginResult, McpConfig, OperationLog, Repo } from "./types";

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
  discoverRepos(token: string) {
    return invoke<Repo[]>("discover_repos", { token });
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
};
