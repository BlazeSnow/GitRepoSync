// 前端多语言：i18next + react-i18next。
// 词典内嵌于本文件；浏览器语言探测（localStorage 记忆 + navigator 回退）；
// 语言切换时同步后端（set_lang 命令），保证错误提示、日志与同步结果语言一致。
import i18n, { type Resource } from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { invoke } from "@tauri-apps/api/core";

export type Lang = "zh" | "en";

const zh = {
  refresh: "刷新",
  cancel: "取消",
  save: "保存",
  copy: "复制",
  copied: "已复制到剪贴板",
  loginSubtitle: "请登录后继续使用",
  username: "用户名",
  password: "密码",
  remember: "保持登录 30 天",
  signIn: "登录",
  signingIn: "登录中…",
  loginHint: "初始账号 admin，初始密码 admin123",
  navSync: "同步仓库",
  navProviders: "提供商",
  navLogs: "日志",
  navMcp: "MCP",
  navSettings: "设置",
  currentUser: "当前用户：{{user}}",
  signOut: "退出登录",
  startSync: "开始同步",
  startSyncCount: "开始同步（{{count}} 个）",
  stopSync: "停止同步",
  staleAll: "全部仓库",
  staleDays: "{{count}} 天内未同步",
  addRepo: "添加仓库",
  colRepo: "仓库",
  colSource: "源地址",
  colTarget: "目标地址",
  colStatus: "状态",
  colLastSynced: "上次同步",
  statusIdle: "未同步",
  statusRunning: "同步中",
  statusSuccess: "成功",
  statusFailed: "失败",
  statusStopped: "已停止",
  syncEmpty: "暂无仓库，点击右上角“添加仓库”开始",
  availableTitle: "未添加的仓库",
  availableFrom: (platform: string) => `来自主账号（${platform}）`,
  availableEmpty: "主账号下暂无可添加的仓库",
  availableHint: "未添加的仓库来自提供商页面设置的主账号",
  quickAdd: "添加",
  editRepo: "编辑仓库",
  addRepoTitle: "添加仓库",
  repoFlowDesc:
    "同步流程：从源仓库拉取到本地基地址中转（更新 LFS 与 submodule），再推送到目标仓库。目标地址为目标仓库的 Git URL。",
  fieldName: "仓库名称",
  fieldSource: "源地址",
  fieldTarget: "目标地址",
  placeholderName: "my-repo",
  placeholderSource: "https://github.com/user/repo.git",
  placeholderTarget: "https://git.example.com/backup/repo.git",
  deleteRepo: "删除仓库",
  deleteRepoDesc: "确定要删除仓库“{{name}}”吗？仅移除记录，不会删除本地文件。",
  confirmDelete: "删除",
  never: "从不",
  justNow: "刚刚",
  minutesAgo: "{{count}} 分钟前",
  hoursAgo: "{{count}} 小时前",
  daysAgo: "{{count}} 天前",
  providersTitle: "提供商",
  providersDesc: "填入对应平台的 Personal Access Token（PAT），即可列出账户及其组织",
  githubHint: "需要 repo / read:org 权限的 Personal Access Token",
  gitlabHint: "需要 read_api / read_user 权限的 Personal Access Token",
  setPrimary: "设为主账号",
  primaryBadge: "主账号",
  patConfigured: "已配置 {{mask}}",
  patPlaceholderNew: "粘贴 Personal Access Token",
  patPlaceholderSaved: "已保存，输入新值可更新",
  saveAndFetch: "保存并获取账户",
  fetching: "获取中…",
  fetchOk: "验证成功",
  orgs: "组织（{{count}}）",
  noOrgs: "无组织",
  logsTitle: "日志",
  logsRecent: "按时间倒序，最近 500 条",
  colTime: "操作时间",
  colAction: "操作",
  colOperator: "操作人",
  logsEmpty: "暂无操作记录",
  mcpTitle: "MCP",
  mcpConnConfig: "连接配置",
  mcpConnDesc: "Agent 通过 MCP stdio 方式连接本软件（APIKEY 鉴权），可管理仓库并触发同步",
  mcpApiKey: "API Key",
  mcpExample: "MCP 客户端配置示例",
  mcpExampleDesc:
    "将以上配置加入 MCP 客户端后，Agent 以子进程方式运行本程序的 mcp 模式（stdio 通信）。",
  mcpTools: "可用工具",
  mcpToolsDesc: "MCP 提供以下工具供 Agent 调用",
  mcpColTool: "工具名",
  mcpColDesc: "说明",
  toolListRepos: "列出所有已配置的同步仓库及其最近一次同步状态",
  toolAddRepo:
    "新增一个同步仓库：从源仓库拉取到本地基地址作为中转站（更新 LFS 与 submodule），再推送到目标仓库地址",
  toolRemoveRepo: "删除指定的同步仓库",
  toolSyncRepo: "立即开始同步指定仓库（异步执行）",
  toolGetSyncStatus: "查询所有仓库的最近同步状态",
  toolGetBaseDir: "查询本地仓库基地址（中转站目录）",
  toolSetBaseDir: "修改本地仓库基地址（中转站目录）",
  settingsTitle: "设置",
  baseDirTitle: "仓库基地址",
  baseDirDesc:
    "同步时从源仓库拉取到基地址下的本地中转目录（按仓库名建目录），更新 LFS 与 submodule 后推送到目标仓库。支持 ~ 开头的路径。",
  currentBaseDir: "当前基地址：{{dir}}",
  baseDirSaved: "基地址已保存",
  account: "账户",
  oldPassword: "旧密码",
  newPassword: "新密码",
  confirmPassword: "确认新密码",
  changePassword: "修改密码",
  pwdMismatch: "两次输入的新密码不一致",
  pwdChanged: "密码修改成功",
  softwareInfo: "软件信息",
  appName: "软件名称",
  versionLabel: "版本号",
  osLabel: "操作系统",
  repoCountLabel: "本软件仓库",
  repoListTitle: "仓库列表",
  repoListDesc: "本软件管理的全部同步仓库（只读，可在“同步仓库”页面编辑）",
  listEmpty: "暂无仓库",
  restoring: "正在恢复登录…",
};

const en = {
  refresh: "Refresh",
  cancel: "Cancel",
  save: "Save",
  copy: "Copy",
  copied: "Copied to clipboard",
  loginSubtitle: "Sign in to continue",
  username: "Username",
  password: "Password",
  remember: "Stay signed in for 30 days",
  signIn: "Sign in",
  signingIn: "Signing in…",
  loginHint: "Initial account: admin, initial password: admin123",
  navSync: "Repositories",
  navProviders: "Providers",
  navLogs: "Logs",
  navMcp: "MCP",
  navSettings: "Settings",
  currentUser: "Signed in as {{user}}",
  signOut: "Sign out",
  startSync: "Start sync",
  startSyncCount: "Start sync ({{count}})",
  stopSync: "Stop sync",
  staleAll: "All repositories",
  staleDays_other: "Not synced in {{count}} days",
  staleDays_one: "Not synced in {{count}} day",
  addRepo: "Add repository",
  colRepo: "Repository",
  colSource: "Source",
  colTarget: "Target",
  colStatus: "Status",
  colLastSynced: "Last synced",
  statusIdle: "Not synced",
  statusRunning: "Syncing",
  statusSuccess: "Success",
  statusFailed: "Failed",
  statusStopped: "Stopped",
  syncEmpty: "No repositories yet — click “Add repository” to get started",
  availableTitle: "Not added repositories",
  availableFrom: (platform: string) => `From primary account (${platform})`,
  availableEmpty: "No repositories available from the primary account",
  availableHint: "Not-added repositories come from the primary account set on the Providers page",
  quickAdd: "Add",
  editRepo: "Edit repository",
  addRepoTitle: "Add repository",
  repoFlowDesc:
    "Sync pipeline: pull from the source into the local base directory (updating LFS and submodules), then push to the target repository. The target is a Git URL.",
  fieldName: "Repository name",
  fieldSource: "Source URL",
  fieldTarget: "Target URL",
  placeholderName: "my-repo",
  placeholderSource: "https://github.com/user/repo.git",
  placeholderTarget: "https://git.example.com/backup/repo.git",
  deleteRepo: "Delete repository",
  deleteRepoDesc: "Delete repository “{{name}}”? Only the record is removed; local files are kept.",
  confirmDelete: "Delete",
  never: "Never",
  justNow: "Just now",
  minutesAgo: "{{count}} min ago",
  hoursAgo: "{{count}} h ago",
  daysAgo: "{{count}} d ago",
  providersTitle: "Providers",
  providersDesc:
    "Enter the Personal Access Token (PAT) for a platform to list your account and organizations",
  githubHint: "A Personal Access Token with repo / read:org scopes",
  gitlabHint: "A Personal Access Token with read_api / read_user scopes",
  setPrimary: "Set as primary",
  primaryBadge: "Primary",
  patConfigured: "Configured {{mask}}",
  patPlaceholderNew: "Paste a Personal Access Token",
  patPlaceholderSaved: "Saved — enter a new value to update",
  saveAndFetch: "Save & fetch account",
  fetching: "Fetching…",
  fetchOk: "Verified",
  orgs: "Organizations ({{count}})",
  noOrgs: "No organizations",
  logsTitle: "Logs",
  logsRecent: "Newest first, latest 500 entries",
  colTime: "Time",
  colAction: "Action",
  colOperator: "Operator",
  logsEmpty: "No operations recorded",
  mcpTitle: "MCP",
  mcpConnConfig: "Connection",
  mcpConnDesc:
    "Agents connect to this app over MCP stdio (API key auth) to manage repositories and trigger syncs",
  mcpApiKey: "API Key",
  mcpExample: "MCP client configuration example",
  mcpExampleDesc:
    "Add the configuration above to your MCP client; the agent runs this app's mcp mode as a child process (stdio).",
  mcpTools: "Available tools",
  mcpToolsDesc: "Tools exposed by MCP for agents",
  mcpColTool: "Tool",
  mcpColDesc: "Description",
  toolListRepos: "List all configured sync repositories with their latest sync status",
  toolAddRepo:
    "Add a sync repository: pull from the source into the local base directory as a staging copy (updating LFS and submodules), then push to the target repository",
  toolRemoveRepo: "Delete the specified sync repository",
  toolSyncRepo: "Start syncing the specified repository immediately (async)",
  toolGetSyncStatus: "Query the latest sync status of all repositories",
  toolGetBaseDir: "Query the local repository base directory (staging directory)",
  toolSetBaseDir: "Change the local repository base directory (staging directory)",
  settingsTitle: "Settings",
  baseDirTitle: "Repository base directory",
  baseDirDesc:
    "During sync, repositories are pulled from the source into a staging directory under the base directory (one folder per repository name); LFS and submodules are updated, then everything is pushed to the target repository. Paths may start with ~.",
  currentBaseDir: "Current base directory: {{dir}}",
  baseDirSaved: "Base directory saved",
  account: "Account",
  oldPassword: "Old password",
  newPassword: "New password",
  confirmPassword: "Confirm new password",
  changePassword: "Change password",
  pwdMismatch: "The two new passwords do not match",
  pwdChanged: "Password changed",
  softwareInfo: "App info",
  appName: "App name",
  versionLabel: "Version",
  osLabel: "Operating system",
  repoCountLabel: "Managed repositories",
  repoListTitle: "Repository list",
  repoListDesc:
    "All repositories managed by this app (read-only; edit them on the Repositories page)",
  listEmpty: "No repositories",
  restoring: "Restoring session…",
};

const resources: Resource = {
  zh: { translation: zh },
  en: { translation: en },
};

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: "zh",
    // 先读 localStorage 记忆，再按浏览器语言；zh* 归一为 zh，其余归一为 en
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
      convertDetectedLanguage: (lng: string) =>
        lng.toLowerCase().startsWith("zh") ? "zh" : lng.toLowerCase().startsWith("en") ? "en" : lng,
    },
    interpolation: { escapeValue: false },
  });

// 语言切换时同步后端（错误提示、日志、同步结果跟随界面语言）
i18n.on("languageChanged", (lng) => {
  const lang: Lang = lng.toLowerCase().startsWith("zh") ? "zh" : "en";
  invoke("set_lang", { lang }).catch(() => {});
});

/** 切换界面语言并同步后端 */
export async function changeAppLang(lang: Lang): Promise<void> {
  await i18n.changeLanguage(lang);
}

/** 相对时间格式化（跟随界面语言） */
export function relativeTime(ms: number | null): string {
  if (ms === null || ms === undefined) return i18n.t("never");
  const diff = Date.now() - ms;
  if (diff <= 0) return i18n.t("justNow");
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return i18n.t("justNow");
  const min = Math.floor(sec / 60);
  if (min < 60) return i18n.t("minutesAgo", { count: min });
  const hour = Math.floor(min / 60);
  if (hour < 24) return i18n.t("hoursAgo", { count: hour });
  const day = Math.floor(hour / 24);
  if (day < 30) return i18n.t("daysAgo", { count: day });
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
