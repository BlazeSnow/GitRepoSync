# Git Repo Sync 更新日志

## 🏷️ v1.0.0-beta.1

### ✨ 新增

- 🖥️ 跨平台桌面应用（Windows / macOS / Linux）：基于 Tauri 2，Rust 后端 + React + TypeScript + shadcn/ui 风格界面
- 🗄️ SQLite 数据库：用户、会话、仓库、设置与操作日志全部入库单个 `app.db`（WAL 模式）；首次启动自动建表并初始化
- 🔄 仓库同步流水线（本地基地址中转）：从源仓库拉取到本地基地址下的中转目录（如 `~/repo/仓库名`），更新中转站的 LFS 与 submodule，再推送分支与标签到目标仓库地址；基地址可在设置页修改（默认 `~/repo`，支持 `~` 路径）
- 🔐 用户登录：初始账号 `admin` / 初始密码 `admin123`（首次启动自动创建），支持「保持登录 30 天」的持久化登录，重启后自动恢复登录态；密码可在设置页修改（需验证旧密码）
- 🖲️ 同步仓库页面：顶部操作栏提供「开始同步 / 停止同步」按钮与同步范围下拉选框（全部 / 1 / 3 / 7 / 30 天内未同步，含从未同步的仓库）；表格展示仓库、源地址、目标地址、状态（同步中 / 成功 / 失败 / 已停止）与上次同步时间；双击或右键表格行弹窗编辑，右键菜单支持编辑、立即同步、删除；「停止同步」可直接终止 git 子进程；同步期间禁用 git 交互式凭据输入，避免私有仓库卡住
- 📜 日志页面：按时间倒序列出软件操作历史（操作、操作人、操作时间），软件全部操作入库 SQLite，界面操作记录登录用户名、Agent 操作记录 `mcp`
- 🔌 MCP 页面：展示 MCP 连接配置（API Key、可一键复制的客户端配置示例）与全部可用工具说明，帮助用户理解与配置接入
- 🤖 Agent（MCP 接入）：软件采用 MCP stdio 连接方式——Agent 以子进程运行 `git-repo-sync.exe mcp`（换行分隔 JSON-RPC 2.0），通过 APIKEY 鉴权（环境变量 `GIT_REPO_SYNC_API_KEY` 或 `--api-key` 参数）；提供 `list_repos`、`add_repo`、`remove_repo`、`sync_repo`、`get_sync_status`、`get_base_dir`、`set_base_dir` 七个工具；客户端断开后进程等待在途同步完成再退出，不中断同步
- 🌍 多语言支持（简体中文 / English）：后端采用 fluent-i18n，全部文案集中在 Fluent 语言包（`locales/*.ftl`），缺失翻译自动回落中文；前端采用 react-i18next，登录页与侧边栏可切换语言（浏览器语言探测 + localStorage 记忆）；MCP 工具描述、参数描述与运行时消息按 Agent 客户端 initialize 的 `locale` 返回，可用环境变量 `GIT_REPO_SYNC_LANG`（`zh` / `en`）强制指定；工具名称为协议契约不随语言变化
- ⚙️ 设置页面：修改本地仓库基地址、修改账户密码、展示软件版本号与本软件源码仓库
- 🏷️ 版本与发布体系：版本号统一维护在 package.json（`version` / `msiVersion` / `baseVersion`），`version.ps1` 一键同步到 tauri.conf.json、Cargo.toml 与 Cargo.lock；`tag.ps1` 一致性检查后打 tag 触发发布；CI 在 PR 与发布时双重校验版本一致性

### 🚀 改进

- ⬆️ 前后端依赖全量升级：前端 React 18 → 19、Vite 6 → 8（rolldown 打包）、Tailwind CSS 3 → 4（CSS-first 主题配置）、TypeScript 5 → 7、tailwind-merge 2 → 3、@vitejs/plugin-react 4 → 6；后端 sha2 0.10 → 0.11、dirs 5 → 7、rusqlite 0.32 → 0.40、reqwest 0.12 → 0.13（rustls 特性更名），移除未使用的 rand 依赖
- ⚡ 新增 `run.ps1` 快速启动脚本：默认构建后**后台启动** Debug 版（源码有变动时自动增量重建，脚本立即返回不占用终端），`-Dev` 进入热重载开发模式，`-Build` 打包发布版；自动切换终端 UTF-8 编码、检查依赖并在首次运行时安装前端依赖
- 📦 发布流水线：GitHub Actions 四平台矩阵打包（Windows x64、macOS Apple Silicon / Intel、Linux x64），tag 推送后自动发布至 GitHub Releases；`vX.Y.Z-beta.N` 形式的 tag 自动标记为 Prerelease；MSI 版本采用独立发布序列号（第 N 次发布为 1.0.N），规避 MSI 版本不支持 beta 语义的问题，保证新旧版本可覆盖升级
- 🖼️ 图片资源统一由 Git LFS 管理，CI 检出时自动拉取
