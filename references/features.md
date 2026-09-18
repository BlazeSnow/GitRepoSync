# 功能设计

> 返回 [DEVELOPMENT.md](../DEVELOPMENT.md)

本文档记录软件各项功能的设计与实现说明，随功能增加持续更新。每完成一项功能，需同步更新本文档与 CHANGELOG.md。

## 1. 用户登录

- 初始用户 `admin`，初始密码 `admin123`（首次启动自动创建）
- 支持持久化登录：勾选“保持登录 30 天”后会话令牌持久保存，软件重启后自动恢复登录
- 密码可在设置页面修改（需验证旧密码，新密码至少 6 位）

**实现说明**：密码以“随机盐 + SHA-256”存储；会话令牌为 UUID，保存在数据目录 `sessions.json`，30 天过期。

## 2. 同步仓库页面

布局分为顶部操作按钮栏和底部表格区。

**操作按钮栏：**

- 按钮“开始同步”：按所选范围同步
- 按钮“停止同步”：终止正在运行的同步进程
- 下拉选框：选择一定时间内未同步的仓库进行同步（全部 / 1 / 3 / 7 / 30 天未同步，含从未同步的仓库）

**表格区：**

- 表格列：仓库、源地址、目标地址、状态、上次同步时间
- 交互：双击或右键表格行后弹窗编辑；右键菜单含“编辑 / 立即同步 / 删除”

**实现说明**：同步引擎调用系统 git——目标路径不存在时执行 `git clone --mirror <源> <目标>`（镜像备份），已存在时在目标内执行 `git remote update --prune`；“停止同步”通过终止 git 子进程实现。状态变化通过 Tauri 事件 `sync-status` 推送到前端。同步期间 `GIT_TERMINAL_PROMPT=0`，避免私有仓库卡在交互式输入。

## 3. 提供商页面

- 目前支持 GitHub、GitLab
- 用户填入对应平台的 PAT（Personal Access Token）后，列出该账户及其组织
- GitHub PAT 建议 `repo` / `read:org` 权限；GitLab PAT 建议 `read_api` / `read_user` 权限

**实现说明**：GitHub 调用 `api.github.com`（`Authorization: Bearer <PAT>`），GitLab 调用 `gitlab.com/api/v4`（`PRIVATE-TOKEN` 头）；PAT 保存在本地 `providers.json`，界面仅显示掩码。

## 4. 设置页面

- 修改账户密码
- 列出本软件管理的仓库（只读列表，编辑入口在“同步仓库”页面）
- 列出本软件版本号、名称与运行系统

## 5. Agent（MCP 接入）

- 软件采用 MCP stdio 连接方式供 Agent 接入：Agent 客户端以子进程方式运行本程序的 `mcp` 模式（`git-repo-sync.exe mcp`），协议为换行分隔的 JSON-RPC 2.0（stdin 读入、stdout 输出）
- MCP 连接通过 APIKEY 鉴权：客户端须通过环境变量 `GIT_REPO_SYNC_API_KEY` 或 `--api-key` 参数提供 APIKEY，与设置页生成的 APIKEY 一致方可访问，否则所有请求返回 unauthorized
- API Key 在设置页查看、复制、重新生成；设置页附 MCP 客户端配置示例

**实现说明**：GUI 与 mcp 子命令共享同一份数据目录（系统应用数据目录 + 应用标识符）。

可用工具：

| 工具 | 说明 |
| --- | --- |
| `list_repos` | 列出所有仓库及最近同步状态 |
| `add_repo` | 新增仓库（name / source / target） |
| `remove_repo` | 删除仓库 |
| `sync_repo` | 立即同步指定仓库（异步） |
| `get_sync_status` | 查询所有仓库最近同步状态 |

## 6. 数据存储

所有持久化数据位于系统应用数据目录（Windows 为 `%APPDATA%\com.blazesnow.gitreposync`）：

| 文件 | 内容 |
| --- | --- |
| `users.json` | 用户与密码哈希 |
| `sessions.json` | 登录会话令牌 |
| `repos.json` | 同步仓库列表与状态 |
| `providers.json` | 平台 PAT |
| `settings.json` | MCP APIKEY |
