# 功能设计

> 返回 [DEVELOPMENT.md](../DEVELOPMENT.md)

本文档记录软件各项功能的设计与实现说明，随功能增加持续更新。每完成一项功能，需同步更新本文档与 CHANGELOG.md。

## 1. 软件逻辑（同步流水线）

按 AGENTS.md 定义的三步流水线执行：

1. **拉取**：从源仓库拉取到本地基地址下的工作副本（中转站），目录为 `{基地址}/{仓库名}`，例如 `~/repo/my-repo`
   - 中转目录不存在：`git clone <源地址> <中转目录>`
   - 已存在：`git remote set-url origin <源地址>`（同步源变更）→ `git fetch origin --prune --tags` → `git pull --ff-only`
2. **更新中转站**：
   - LFS：`git lfs pull`（系统未安装 git-lfs 时跳过，并在结果消息中注明）
   - submodule：`git submodule update --init --recursive`
3. **推送**：`git push <目标仓库地址> +refs/heads/*:refs/heads/* +refs/tags/*:refs/tags/*`（镜像分支与标签到目标仓库）

同步过程中每一步的 git 子进程可被“停止同步”终止；同步状态通过 Tauri 事件 `sync-status` 推送到前端。同步期间 `GIT_TERMINAL_PROMPT=0`，避免私有仓库卡在交互式输入。

## 2. 数据库（SQLite）

所有业务数据存储于 SQLite（应用数据目录下 `app.db`，WAL 模式，rusqlite bundled 编译）：

| 表 | 内容 |
| --- | --- |
| `users` | 用户与密码哈希（随机盐 + SHA-256） |
| `sessions` | 登录会话令牌（持久化登录，30 天有效） |
| `repos` | 同步仓库列表与最近同步状态 |
| `providers` | 平台 PAT（GitHub / GitLab） |
| `settings` | 键值设置（`base_dir` 仓库基地址、`mcp_api_key`） |
| `operation_logs` | 软件全部操作历史（操作、操作人、操作时间） |

首次启动自动建表并初始化：初始用户 `admin` / `admin123`、默认基地址 `~/repo`、随机 MCP APIKEY。

## 3. 用户登录

- 初始用户 `admin`，初始密码 `admin123`
- 支持持久化登录：勾选“保持登录 30 天”后会话令牌入库，软件重启后自动恢复登录
- 密码可在设置页面修改（需验证旧密码，新密码至少 6 位）

## 4. 同步仓库页面

布局分为顶部操作按钮栏和底部表格区。

**操作按钮栏：**

- 按钮“开始同步”：按所选范围同步
- 按钮“停止同步”：终止正在运行的同步进程
- 下拉选框：选择一定时间内未同步的仓库进行同步（全部 / 1 / 3 / 7 / 30 天未同步，含从未同步的仓库）

**表格区：**

- 表格列：仓库、源地址、目标地址、状态、上次同步时间
- 交互：双击或右键表格行后弹窗编辑；右键菜单含“编辑 / 立即同步 / 删除”
- 仓库地址含义：源地址与目标地址均为 Git 仓库 URL（本地中转目录由基地址与仓库名派生，无需用户填写）

## 5. 提供商页面

- 目前支持 GitHub、GitLab
- 用户填入对应平台的 PAT（Personal Access Token）后，列出该账户及其组织
- GitHub PAT 建议 `repo` / `read:org` 权限；GitLab PAT 建议 `read_api` / `read_user` 权限

**实现说明**：GitHub 调用 `api.github.com`（`Authorization: Bearer <PAT>`），GitLab 调用 `gitlab.com/api/v4`（`PRIVATE-TOKEN` 头）；PAT 入库 `providers` 表，界面仅显示掩码。

## 6. 日志页面

- 以表格形式按时间倒序列出软件操作历史：操作时间、操作、操作人（默认最近 500 条）
- 软件操作全部入库 sqlite `operation_logs` 表，覆盖：登录 / 登录失败 / 退出登录、修改密码、添加 / 编辑 / 删除仓库、开始 / 停止同步及同步结果、保存 / 清除 PAT、修改基地址、重新生成 MCP APIKEY、MCP 工具调用
- 操作人：界面操作为登录用户名，Agent 调用为 `mcp`

## 7. MCP 页面

- 列出 MCP 连接配置信息（API Key、客户端配置示例），帮助用户理解配置；支持复制与重新生成 APIKEY
- 列出 MCP 可用工具及说明

**实现说明**：软件采用 MCP stdio 连接方式——Agent 客户端以子进程运行本程序的 `mcp` 模式（`git-repo-sync.exe mcp`），协议为换行分隔的 JSON-RPC 2.0（stdin 读入、stdout 输出）。通过 APIKEY 鉴权：客户端经环境变量 `GIT_REPO_SYNC_API_KEY` 或 `--api-key` 参数提供，与 `settings` 表中的 APIKEY 一致方可访问。客户端断开后进程会等待在途同步完成再退出，不会中断同步。GUI 与 mcp 子命令共享同一 SQLite 数据库。

可用工具：

| 工具 | 说明 |
| --- | --- |
| `list_repos` | 列出所有仓库及最近同步状态 |
| `add_repo` | 新增仓库（name / source / target） |
| `remove_repo` | 删除仓库 |
| `sync_repo` | 立即同步指定仓库（异步） |
| `get_sync_status` | 查询所有仓库最近同步状态 |
| `get_base_dir` | 查询本地仓库基地址 |
| `set_base_dir` | 修改本地仓库基地址 |

## 8. 设置页面

- 修改本地仓库基地址（同步中转目录的父目录，默认 `~/repo`，支持 `~` 开头路径）
- 修改账户密码
- 列出本软件管理的仓库（只读列表，编辑入口在“同步仓库”页面）
- 列出本软件版本号、名称与运行系统

## 9. 多语言（中文 / English）

- 后端采用 [fluent-i18n](https://crates.io/crates/fluent-i18n)，全部文案集中在 `src-tauri/locales/zh-CN/main.ftl`（默认与回落语言）与 `en-US/main.ftl`，编译期内嵌
- 前端采用 [react-i18next](https://react.i18next.dev/)（词典内嵌 `src/i18n.ts`），登录页与侧边栏均可切换语言，选择存入 localStorage，首次启动按浏览器语言自动选择
- 语言选择规则：
  - GUI：前端切换语言时通过 `set_lang` 命令同步到后端，之后的错误提示、日志、同步结果按该语言记录
  - MCP：按 initialize 请求的 `locale` 字段返回工具描述与运行时消息；环境变量 `GIT_REPO_SYNC_LANG`（`zh` / `en`）可强制指定
  - 均未提供时默认中文；缺失翻译自动回落中文
  - MCP 工具名称为协议契约，不随语言变化
- 注意：日志与同步结果按操作发生时的语言写入数据库，切换语言不会改写历史记录
