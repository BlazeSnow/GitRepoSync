import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

export type Lang = "zh" | "en";

const zh = {
  // 通用
  refresh: "刷新",
  cancel: "取消",
  save: "保存",
  copy: "复制",
  copied: "已复制到剪贴板",
  langName: "English",
  // 登录
  loginSubtitle: "请登录后继续使用",
  username: "用户名",
  password: "密码",
  remember: "保持登录 30 天",
  signIn: "登录",
  signingIn: "登录中…",
  loginHint: "初始账号 admin，初始密码 admin123",
  // 侧边栏
  navSync: "同步仓库",
  navProviders: "提供商",
  navLogs: "日志",
  navMcp: "MCP",
  navSettings: "设置",
  currentUser: (u: string) => `当前用户：${u}`,
  signOut: "退出登录",
  // 同步仓库
  startSync: "开始同步",
  startSyncCount: (n: number) => `开始同步（${n} 个）`,
  stopSync: "停止同步",
  staleAll: "全部仓库",
  staleDays: (d: number) => `${d} 天内未同步`,
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
  deleteRepoDesc: (name: string) =>
    `确定要删除仓库“${name}”吗？仅移除记录，不会删除本地文件。`,
  confirmDelete: "删除",
  // 相对时间
  never: "从不",
  justNow: "刚刚",
  minutesAgo: (m: number) => `${m} 分钟前`,
  hoursAgo: (h: number) => `${h} 小时前`,
  daysAgo: (d: number) => `${d} 天前`,
  // 提供商
  providersTitle: "提供商",
  providersDesc: "填入对应平台的 Personal Access Token（PAT），即可列出账户及其组织",
  githubHint: "需要 repo / read:org 权限的 Personal Access Token",
  gitlabHint: "需要 read_api / read_user 权限的 Personal Access Token",
  patConfigured: (mask: string) => `已配置 ${mask}`,
  patPlaceholderNew: "粘贴 Personal Access Token",
  patPlaceholderSaved: "已保存，输入新值可更新",
  saveAndFetch: "保存并获取账户",
  fetching: "获取中…",
  fetchOk: "验证成功",
  orgs: (n: number) => `组织（${n}）`,
  noOrgs: "无组织",
  // 日志
  logsTitle: "日志",
  logsRecent: "按时间倒序，最近 500 条",
  colTime: "操作时间",
  colAction: "操作",
  colOperator: "操作人",
  logsEmpty: "暂无操作记录",
  // MCP
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
  toolAddRepo: "新增一个同步仓库：从源仓库拉取到本地基地址作为中转站（更新 LFS 与 submodule），再推送到目标仓库地址",
  toolRemoveRepo: "删除指定的同步仓库",
  toolSyncRepo: "立即开始同步指定仓库（异步执行）",
  toolGetSyncStatus: "查询所有仓库的最近同步状态",
  toolGetBaseDir: "查询本地仓库基地址（中转站目录）",
  toolSetBaseDir: "修改本地仓库基地址（中转站目录）",
  // 设置
  settingsTitle: "设置",
  baseDirTitle: "仓库基地址",
  baseDirDesc:
    "同步时从源仓库拉取到基地址下的本地中转目录（按仓库名建目录），更新 LFS 与 submodule 后推送到目标仓库。支持 ~ 开头的路径。",
  currentBaseDir: (dir: string) => `当前基地址：${dir}`,
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

export type Dict = typeof zh;

const en: Dict = {
  refresh: "Refresh",
  cancel: "Cancel",
  save: "Save",
  copy: "Copy",
  copied: "Copied to clipboard",
  langName: "中文",
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
  currentUser: (u) => `Signed in as ${u}`,
  signOut: "Sign out",
  startSync: "Start sync",
  startSyncCount: (n) => `Start sync (${n})`,
  stopSync: "Stop sync",
  staleAll: "All repositories",
  staleDays: (d) => `Not synced in ${d} days`,
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
  deleteRepoDesc: (name) =>
    `Delete repository “${name}”? Only the record is removed; local files are kept.`,
  confirmDelete: "Delete",
  never: "Never",
  justNow: "Just now",
  minutesAgo: (m) => `${m} min ago`,
  hoursAgo: (h) => `${h} h ago`,
  daysAgo: (d) => `${d} d ago`,
  providersTitle: "Providers",
  providersDesc:
    "Enter the Personal Access Token (PAT) for a platform to list your account and organizations",
  githubHint: "A Personal Access Token with repo / read:org scopes",
  gitlabHint: "A Personal Access Token with read_api / read_user scopes",
  patConfigured: (mask) => `Configured ${mask}`,
  patPlaceholderNew: "Paste a Personal Access Token",
  patPlaceholderSaved: "Saved — enter a new value to update",
  saveAndFetch: "Save & fetch account",
  fetching: "Fetching…",
  fetchOk: "Verified",
  orgs: (n) => `Organizations (${n})`,
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
  currentBaseDir: (dir) => `Current base directory: ${dir}`,
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

const DICTS: Record<Lang, Dict> = { zh, en };

function detectLang(): Lang {
  const saved = localStorage.getItem("grs_lang");
  if (saved === "zh" || saved === "en") return saved;
  return navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en";
}

interface I18n {
  lang: Lang;
  setLang: (l: Lang) => void;
  t: Dict;
}

const I18nContext = createContext<I18n>({
  lang: "zh",
  setLang: () => {},
  t: zh,
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [lang, setLangState] = useState<Lang>(detectLang);

  useEffect(() => {
    invoke("set_lang", { lang }).catch(() => {});
  }, []); // 启动时同步后端语言

  function setLang(l: Lang) {
    setLangState(l);
    localStorage.setItem("grs_lang", l);
    invoke("set_lang", { lang: l }).catch(() => {});
  }

  return (
    <I18nContext.Provider value={{ lang, setLang, t: DICTS[lang] }}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n(): I18n {
  return useContext(I18nContext);
}

/** 语言切换按钮：显示另一种语言的名称 */
export function LangButton() {
  const { lang, setLang } = useI18n();
  return (
    <button
      type="button"
      className="rounded-md border px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
      onClick={() => setLang(lang === "zh" ? "en" : "zh")}
    >
      {lang === "zh" ? "English" : "中文"}
    </button>
  );
}

/** 相对时间格式化（跟随界面语言） */
export function relativeTime(ms: number | null, lang: Lang): string {
  const t = DICTS[lang];
  if (ms === null || ms === undefined) return t.never;
  const diff = Date.now() - ms;
  if (diff <= 0) return t.justNow;
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return t.justNow;
  const min = Math.floor(sec / 60);
  if (min < 60) return t.minutesAgo(min);
  const hour = Math.floor(min / 60);
  if (hour < 24) return t.hoursAgo(hour);
  const day = Math.floor(hour / 24);
  if (day < 30) return t.daysAgo(day);
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
