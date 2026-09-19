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
  colAddress: "地址",
  colTarget: "目标地址",
  colStatus: "状态",
  colLastSynced: "上次同步",
  notConfigured: "未配置",
  statusIdle: "未同步",
  statusRunning: "同步中",
  statusSuccess: "成功",
  statusFailed: "失败",
  statusStopped: "已停止",
  syncEmpty: "暂无仓库，点击右上角“添加仓库”开始",
  loading: "加载中…",
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
  never: "从未",
  justNow: "刚刚",
  minutesAgo: "{{count}} 分钟前",
  hoursAgo: "{{count}} 小时前",
  daysAgo: "{{count}} 天前",
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
  toolDiscoverRepos:
    "扫描基地址（中转站目录）内的一级子目录，把其中的 git 仓库登记进列表：origin 远端作为源地址、其余全部远端作为备份目标；返回登记后的完整仓库列表",
  toolAddRepo:
    "新增或更新同步仓库（按名称幂等）：同名仓库已存在时更新源地址、合并目标并重新登记，不会产生重复条目；同步时从源仓库拉取到本地基地址作为中转站（更新 LFS 与 submodule），再推送到目标仓库地址",
  toolUpdateRepo:
    "更新指定同步仓库的配置：可修改名称与源地址；提供 targets 时整体替换目标列表（仅补充目标请用 add_repo，其为合并语义）",
  toolRemoveRepo:
    "移除指定同步仓库（软删除）：从列表隐藏并清空其目标配置；基地址内目录不被删除，也不会被自动发现重新登记",
  toolSyncRepo: "立即开始同步指定仓库（异步执行）",
  toolSyncRepos:
    "批量触发同步：提供 ids 按列表同步，或提供 days 按范围同步（0=全部仓库，N=最近 N 天未同步，含从未同步，与界面范围一致）；未配置（缺源地址或目标）的仓库自动跳过，两者都提供时优先 ids。异步执行，可用 get_sync_status 查询进度",
  toolGetSyncStatus: "查询所有仓库的最近同步状态",
  toolListLogs: "按时间倒序列出软件操作日志（操作、操作人、操作时间）",
  toolGetBaseDir: "查询本地仓库基地址（中转站目录）",
  toolSetBaseDir: "修改本地仓库基地址（中转站目录）",
  settingsTitle: "设置",
  langDesc: "切换界面语言，立即生效",
  appearanceTitle: "外观",
  themeDesc: "跟随系统深浅色显示，也可手动指定",
  themeSystem: "跟随系统",
  themeLight: "浅色",
  themeDark: "深色",
  baseDirTitle: "仓库基地址",
  baseDirDesc:
    "同步时从源仓库拉取到基地址下的本地中转目录（按仓库名建目录），更新 LFS 与 submodule 后推送到目标仓库。点击下方按钮选择目录（以完整路径保存）。",
  baseDirSaved: "基地址已保存",
  browse: "选择目录…",
  account: "账户",
  oldPassword: "旧密码",
  newPassword: "新密码",
  confirmPassword: "确认新密码",
  changePassword: "修改密码",
  pwdMismatch: "两次输入的新密码不一致",
  pwdChanged: "密码修改成功",
  softwareInfo: "软件信息",
  versionLabel: "版本号",
  repoLinkLabel: "本软件仓库",
  openRepoLink: "在系统浏览器中打开",
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
  colAddress: "Addresses",
  colTarget: "Target",
  colStatus: "Status",
  colLastSynced: "Last synced",
  notConfigured: "Not configured",
  statusIdle: "Not synced",
  statusRunning: "Syncing",
  statusSuccess: "Success",
  statusFailed: "Failed",
  statusStopped: "Stopped",
  syncEmpty: "No repositories yet — click “Add repository” to get started",
  loading: "Loading…",
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
  toolDiscoverRepos:
    "Scan the first-level subdirectories of the base directory (staging directory) and register the git repositories found: the origin remote becomes the source, all other remotes become backup targets; returns the full repository list after registration",
  toolAddRepo:
    "Add or update a sync repository (idempotent by name): when a repository with the same name exists, its source is updated, targets are merged and it is re-registered instead of creating a duplicate entry; syncing pulls from the source into the local base directory as a staging copy (updating LFS and submodules), then pushes to the target repository",
  toolUpdateRepo:
    "Update the configuration of the specified sync repository: rename and change the source URL; when targets is provided it fully replaces the target list (to only add targets use add_repo, which merges)",
  toolRemoveRepo:
    "Remove the specified sync repository (soft delete): it is hidden from the list and its targets are cleared; the directory under the base directory is not deleted and the repository will not be re-discovered",
  toolSyncRepo: "Start syncing the specified repository immediately (async)",
  toolSyncRepos:
    "Trigger batch syncs: provide ids to sync a list, or days for a range (0 = all repositories, N = not synced within the last N days, including never synced, matching the app's range); unconfigured repositories are skipped, ids takes precedence when both are given. Async; use get_sync_status to check progress",
  toolGetSyncStatus: "Query the latest sync status of all repositories",
  toolListLogs: "List the operation log newest first (action, operator, time)",
  toolGetBaseDir: "Query the local repository base directory (staging directory)",
  toolSetBaseDir: "Change the local repository base directory (staging directory)",
  settingsTitle: "Settings",
  langDesc: "Switch the interface language, takes effect immediately",
  appearanceTitle: "Appearance",
  themeDesc: "Follow the system light/dark appearance, or force one manually",
  themeSystem: "System",
  themeLight: "Light",
  themeDark: "Dark",
  baseDirTitle: "Repository base directory",
  baseDirDesc:
    "During sync, repositories are pulled from the source into a staging directory under the base directory (one folder per repository name); LFS and submodules are updated, then everything is pushed to the target repository. Pick a folder with the button below (stored as a full path).",
  baseDirSaved: "Base directory saved",
  browse: "Choose folder…",
  account: "Account",
  oldPassword: "Old password",
  newPassword: "New password",
  confirmPassword: "Confirm new password",
  changePassword: "Change password",
  pwdMismatch: "The two new passwords do not match",
  pwdChanged: "Password changed",
  softwareInfo: "App info",
  versionLabel: "Version",
  repoLinkLabel: "Source repository",
  openRepoLink: "Open in system browser",
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
