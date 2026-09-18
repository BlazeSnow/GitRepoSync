# Git Repo Sync 后端文案（中文，默认与回落语言）。
# 键为 kebab-case；带参用 {$name} 占位。英文包为 locales/en-US/main.ftl。

# ---------- 通用 ----------
db-error = 数据库错误：{$err}
join-sep = ；

# ---------- 认证 ----------
invalid-credentials = 用户名或密码错误
login-failed-log = 登录失败（用户名或密码错误）
session-expired = 登录已失效，请重新登录
new-password-too-short = 新密码至少需要 6 个字符
user-not-found = 用户不存在
wrong-password = 旧密码不正确
log-login = 登录
log-logout = 退出登录
log-change-password = 修改密码

# ---------- 仓库 ----------
repo-fields-empty = 仓库名称、源地址、目标地址均不能为空
repo-not-found = 仓库不存在
repo-syncing = 该仓库正在同步，请先停止同步
log-repo-added = 添加仓库「{$name}」
log-repo-edited = 编辑仓库「{$name}」
log-repo-deleted = 删除仓库「{$name}」

# ---------- 同步 ----------
log-sync-started = 开始同步仓库「{$name}」
log-sync-success = 同步仓库「{$name}」成功
log-sync-failed = 同步仓库「{$name}」失败
log-sync-stopped = 同步仓库「{$name}」已停止
log-sync-stopped-cmd = 停止同步
step-clone = 从源仓库克隆到本地中转站
step-fetch = 拉取源仓库更新
step-lfs = 更新 LFS 文件
step-submodule = 更新 submodule
step-push = 推送到目标仓库
lfs-skipped = 未检测到 git-lfs，已跳过 LFS 更新
manually-stopped = 已手动停止
git-spawn-error = 无法启动 git：{$err}（请确认系统已安装 Git 并加入 PATH）
git-wait-error = 等待 git 进程失败
process-terminated = 进程已被终止


# ---------- 设置 ----------
base-dir-empty = 基地址不能为空
log-base-dir-changed = 修改仓库基地址为 {$dir}
log-mcp-key-regenerated = 重新生成 MCP APIKEY

# ---------- MCP ----------
mcp-unauthorized = unauthorized: APIKEY 不正确
mcp-parse-error = parse error
mcp-method-not-found = method not found: {$method}
mcp-unknown-tool = unknown tool: {$tool}
mcp-spawn-internal = 内部错误
log-mcp-repo-added = 通过 MCP 添加仓库「{$name}」
log-mcp-repo-deleted = 通过 MCP 删除仓库
log-mcp-sync = 通过 MCP 触发同步
log-mcp-base-dir-changed = 通过 MCP 修改仓库基地址为 {$dir}

# ---------- MCP 工具描述 ----------
tool-list-repos = 列出所有已配置的同步仓库及其最近一次同步状态
tool-add-repo = 新增一个同步仓库：从源仓库拉取到本地基地址作为中转站（更新 LFS 与 submodule），再推送到目标仓库地址
tool-add-repo-name = 仓库名称（同时是本地基地址下的目录名）
tool-add-repo-source = 源地址（Git 仓库 URL）
tool-add-repo-target = 目标仓库地址（Git 仓库 URL）
tool-remove-repo = 删除指定的同步仓库
tool-remove-repo-id = 仓库 ID
tool-sync-repo = 立即开始同步指定仓库（异步执行，可用 get_sync_status 查询进度）
tool-sync-repo-id = 仓库 ID
tool-get-sync-status = 查询所有仓库的最近同步状态
tool-get-base-dir = 查询本地仓库基地址（中转站目录）
tool-set-base-dir = 修改本地仓库基地址（中转站目录）
tool-set-base-dir-base-dir = 基地址路径，支持 ~ 开头
