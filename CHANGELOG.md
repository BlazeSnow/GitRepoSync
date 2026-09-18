# 更新日志

本项目的所有重要变更将记录在本文件。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)。

## [Unreleased]

## [0.1.0] - 2026-09-18

### 新增

- 基于 Tauri 2 的跨平台桌面应用（Windows / macOS / Linux），后端 Rust，前端 React + TypeScript + shadcn/ui 风格组件
- 用户登录：初始用户 `admin` / 初始密码 `admin123`，支持“保持登录 30 天”的持久化登录，登录状态跨重启恢复
- 同步仓库页面：
  - 顶部操作栏：“开始同步”“停止同步”按钮，以及“一定时间内未同步”范围下拉选框（全部 / 1 / 3 / 7 / 30 天）
  - 表格展示仓库、源地址、目标地址、状态、上次同步时间
  - 双击或右键表格行弹窗编辑；右键菜单支持编辑、立即同步、删除
  - 同步引擎基于系统 git：目标不存在时 `git clone --mirror`，已存在时 `git remote update --prune`
- 提供商页面：支持 GitHub、GitLab，填入 PAT 后列出账户及其组织
- 设置页面：修改账户密码、列出本软件管理的仓库、显示软件版本号与运行系统
- Agent 接入：内置 MCP 服务（HTTP，默认 `http://127.0.0.1:17878/mcp`），APIKEY 鉴权，提供 list_repos / add_repo / remove_repo / sync_repo / get_sync_status 工具；API Key 可在设置页复制与重新生成
- GitHub Actions 自动打包发布至 GitHub Releases，`vX.Y.Z-beta.N` 形式的 tag 触发 pre-release（beta 版本）
- 图片类文件统一由 Git LFS 管理

[Unreleased]: https://github.com/BlazeSnow/GitRepoSync/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/BlazeSnow/GitRepoSync/releases/tag/v0.1.0
