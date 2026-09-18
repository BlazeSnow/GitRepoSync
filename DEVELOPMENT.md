# DEVELOPMENT.md — Git Repo Sync 开发文档

本文档是开发文档的主入口，详细内容拆分至 [references/](./references/) 目录。开发人员须同时遵守 [AGENTS.md](./AGENTS.md) 中的约定。

> 按照约定：每完成一项功能，需同步更新本文件（及相关 references 文档）与 CHANGELOG.md。

## 1. 项目简介

Git Repo Sync 是一款跨平台的 Git 仓库同步桌面软件，用于将仓库从源地址同步到目标地址，并支持通过 Agent（MCP）进行自动化操作。

## 2. 技术栈

| 层级 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 后端 | Rust |
| 前端 UI | shadcn/ui（React + TypeScript + Tailwind CSS） |
| 打包发布 | GitHub Actions → GitHub Releases（支持 beta 版本） |

## 3. 文档索引

| 文档 | 内容 |
| --- | --- |
| [environment.md](./references/environment.md) | 环境准备与常用命令：基础依赖、各平台系统依赖、终端编码（GBK 与 UTF-8）处理 |
| [features.md](./references/features.md) | 功能设计：用户登录、同步仓库页面、提供商页面、设置页面、Agent（MCP）接入 |
| [release.md](./references/release.md) | 版本号规则与发布流程：version.ps1 / tag.ps1、GitHub Releases、beta 版本 |

## 4. 快速开始

```powershell
.\run.ps1               # 快速启动 Debug 版（源码有改动时自动增量重建）
.\run.ps1 -Dev          # 开发模式（前端热重载 + Rust 增量编译）
.\run.ps1 -Build        # 打包发布版安装包
```

脚本会自动切换终端为 UTF-8、检查依赖并在首次运行时安装前端依赖。等价的原始命令：

```bash
pnpm install            # 安装前端依赖
pnpm tauri dev          # 以开发模式启动（前端热重载 + Rust 增量编译）
pnpm tauri build        # 构建发布版安装包
```

环境搭建与更多命令见 [references/environment.md](./references/environment.md)。

## 5. 项目结构

```
├── .github/workflows/   # CI：release.yml（打 tag 发布）+ check.yml（PR 版本校验）
├── ci/                  # CI 辅助脚本（版本一致性校验、Linux 依赖）
├── AGENTS.md            # 开发/Agent 约定（禁止修改）
├── CHANGELOG.md         # 变更记录（每完成一项功能后更新）
├── DEVELOPMENT.md       # 开发文档主入口
├── version.ps1          # 版本号同步（package.json → tauri.conf / Cargo）
├── tag.ps1              # 版本一致性检查 + 打 tag 发布
├── run.ps1              # 快速启动（Debug 运行 / 开发模式 / 打包）
├── references/          # 开发文档详细内容
│   ├── environment.md   # 环境准备与常用命令
│   ├── features.md      # 功能设计与实现说明
│   └── release.md       # 版本号规则与发布流程
├── src/                 # 前端代码（React + TypeScript）
│   ├── components/      # 页面组件与 shadcn/ui 风格基础组件（ui/）
│   ├── lib/             # Tauri 命令封装（api.ts）、类型、工具函数
│   └── main.tsx
├── src-tauri/           # Rust 后端
│   ├── src/             # main / state / auth / repos / providers / settings / mcp
│   ├── Cargo.toml
│   └── tauri.conf.json
├── index.html
├── package.json
├── vite.config.ts
└── tailwind.config.js
```

## 6. 开发约定

1. [AGENTS.md](./AGENTS.md) 禁止修改。
2. 每完成一项功能，同步更新 CHANGELOG.md 与本文件（及相关 references 文档）。
3. 注意终端 GBK 与 UTF-8 编码问题，详见 [references/environment.md](./references/environment.md)。
4. 跨平台要求：所有功能需在 Windows、macOS、Linux 上可用；路径处理使用跨平台 API（如 `std::path::Path`），禁止硬编码路径分隔符。
5. 分支约定：日常开发在 `dev` 分支进行，功能稳定后合入 `main` 分支。
