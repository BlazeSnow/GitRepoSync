import { invoke } from "@tauri-apps/api/core";
import type {
  AccountInfo,
  AppInfo,
  LoginResult,
  McpConfig,
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
  setMcpPort(token: string, port: number) {
    return invoke<void>("set_mcp_port", { token, port });
  },
  regenerateMcpKey(token: string) {
    return invoke<string>("regenerate_mcp_api_key", { token });
  },
};
