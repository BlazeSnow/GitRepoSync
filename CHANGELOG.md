# Git Repo Sync 更新日志

## 🏷️ v1.0.0-beta.1

### ✨ 新增

- 🖥️ 跨平台桌面应用（Windows / macOS / Linux）：基于 Tauri 2，Rust 后端 + React + TypeScript + shadcn/ui 风格界面
- 🔐 用户登录：初始账号 `admin` / 初始密码 `admin123`（首次启动自动创建），支持「保持登录 30 天」的持久化登录，重启后自动恢复登录态；密码可在设置页修改（需验证旧密码）
- 🔄 同步仓库页面：顶部操作栏提供「开始同步 / 停止同步」按钮与同步范围下拉选框（全部 / 1 / 3 / 7 / 30 天内未同步，含从未同步的仓库）；表格展示仓库、源地址、目标地址、状态（同步中 / 成功 / 失败 / 已停止）与上次同步时间；双击或右键表格行弹窗编辑，右键菜单支持编辑、立即同步、删除；同步引擎调用系统 git——目标不存在时 `git clone --mirror` 镜像备份，已存在时 `git remote update --prune` 增量更新，「停止同步」直接终止 git 进程；同步期间禁用 git 交互式凭据输入，避免私有仓库卡住
- 🏢 提供商页面：支持 GitHub、GitLab，填入对应平台的 Personal Access Token（PAT）即可列出账户及其组织；PAT 仅保存在本地数据目录，界面只显示掩码
- ⚙️ 设置页面：修改账户密码；列出本软件管理的全部仓库；展示软件名称、版本号与运行系统
- 🤖 Agent（MCP 接入）：内置 MCP 服务（HTTP + JSON-RPC 2.0，默认 `http://127.0.0.1:17878/mcp`），通过 APIKEY 鉴权（`Authorization: Bearer` 或 `X-Api-Key`）；提供 `list_repos`、`add_repo`、`remove_repo`、`sync_repo`、`get_sync_status` 五个工具，Agent 可直接管理仓库并触发同步；API Key 在设置页查看、复制与一键重新生成，服务端口可修改（重启生效）
- 🏷️ 版本与发布体系：版本号统一维护在 package.json（`version` / `msiVersion` / `baseVersion`），`version.ps1` 一键同步到 tauri.conf.json、Cargo.toml 与 Cargo.lock；`tag.ps1` 一致性检查后打 tag 触发发布；CI 在 PR 与发布时双重校验版本一致性

### 🚀 改进

- ⚡ 新增 `run.ps1` 快速启动脚本：默认直接运行 Debug 版（源码有变动时自动增量重建），`-Dev` 进入热重载开发模式，`-Build` 打包发布版；自动切换终端 UTF-8 编码、检查依赖并在首次运行时安装前端依赖
- 📦 发布流水线：GitHub Actions 四平台矩阵打包（Windows x64、macOS Apple Silicon / Intel、Linux x64），tag 推送后自动发布至 GitHub Releases；`vX.Y.Z-beta.N` 形式的 tag 自动标记为 Prerelease；MSI 版本采用独立发布序列号（第 N 次发布为 1.0.N），规避 MSI 版本不支持 beta 语义的问题，保证新旧版本可覆盖升级
- 🖼️ 图片资源统一由 Git LFS 管理，CI 检出时自动拉取
