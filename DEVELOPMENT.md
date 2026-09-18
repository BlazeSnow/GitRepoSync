# DEVELOPMENT.md — Git Repo Sync 开发文档

本文档描述 Git Repo Sync 的开发环境、开发规范与功能设计。开发人员须同时遵守 [AGENTS.md](./AGENTS.md) 中的约定。

> 按照约定：每完成一项功能，需同步更新本文件与 [CHANGELOG.md](./CHANGELOG.md)。

## 1. 项目简介

Git Repo Sync 是一款跨平台的 Git 仓库同步桌面软件，用于将仓库从源地址同步到目标地址，并支持通过 Agent（MCP）进行自动化操作。

## 2. 技术栈

| 层级 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 后端 | Rust |
| 前端 UI | shadcn/ui（React + TypeScript + Tailwind CSS） |
| 打包发布 | GitHub Actions → GitHub Releases（支持 beta 版本） |

## 3. 环境准备

### 3.1 基础依赖

- **Rust**（stable，通过 [rustup](https://rustup.rs) 安装）
- **Node.js** ≥ 20 LTS
- **包管理器**：pnpm（或 npm）
- **Tauri CLI**：随前端依赖安装 `@tauri-apps/cli`，或全局安装 `cargo install tauri-cli --version "^2"`

### 3.2 各平台系统依赖

- **Windows**：WebView2 Runtime（Windows 10/11 一般自带）、Visual Studio Build Tools（含 "使用 C++ 的桌面开发" 工作负载）
- **macOS**：Xcode Command Line Tools（`xcode-select --install`）
- **Linux（Debian/Ubuntu）**：
  ```bash
  sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
    libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```

### 3.3 终端编码（Windows 重点）

AGENTS.md 要求开发过程中处理终端 GBK 与 UTF-8 的关系：

1. 所有源码、配置与文档文件统一保存为 **UTF-8** 编码。
2. Windows 传统终端（cmd / PowerShell）默认代码页为 GBK（936），运行含中文输出的命令前先切换为 UTF-8：
   - cmd：`chcp 65001`
   - PowerShell：`[Console]::OutputEncoding = [System.Text.Encoding]::UTF8`
3. Git Bash 下通常无需额外处理；若中文显示乱码，检查 `LANG` / `LC_ALL` 是否为 `*.UTF-8`。
4. Rust 后端在 Windows 控制台打印中文日志时，注意输出编码与控制台代码页匹配，避免日志乱码。
5. 涉及仓库路径的存储与传递时，注意非 ASCII（中文）路径的编解码，统一按 UTF-8 处理。

## 4. 常用命令

```bash
pnpm install            # 安装前端依赖
pnpm tauri dev          # 以开发模式启动（前端热重载 + Rust 增量编译）
pnpm tauri build        # 构建发布版安装包

# 在 src-tauri 目录下：
cargo check             # 快速检查 Rust 代码
cargo fmt               # Rust 代码格式化
cargo clippy            # Rust 静态检查
```

## 5. 项目结构（规划）

```
├── AGENTS.md            # 开发/Agent 约定（禁止修改）
├── CHANGELOG.md         # 变更记录（每完成一项功能后更新）
├── DEVELOPMENT.md       # 本文件（开发文档）
├── src/                 # 前端代码（shadcn/ui）
└── src-tauri/           # Rust 后端
    ├── src/
    ├── Cargo.toml
    └── tauri.conf.json
```

## 6. 功能设计

### 6.1 用户登录

- 初始用户 `admin`，初始密码 `admin123`
- 支持持久化登录：登录状态可跨软件重启保留
- 密码可在设置页面修改

### 6.2 同步仓库页面

布局分为顶部操作按钮栏和底部表格区。

**操作按钮栏：**

- 按钮“开始同步”
- 按钮“停止同步”
- 下拉选框：选择一定时间内未同步的仓库进行同步

**表格区：**

- 表格列：仓库、源地址、目标地址
- 交互：双击或右键表格行后弹窗编辑

### 6.3 提供商页面

- 目前支持 GitHub、GitLab
- 用户填入对应平台的 PAT（Personal Access Token）后，列出该账户及其组织

### 6.4 设置页面

- 修改账户密码
- 列出本软件仓库
- 列出本软件版本号

### 6.5 Agent（MCP 接入）

- 软件采用 MCP（Model Context Protocol）连接方式供 Agent 接入
- MCP 连接通过 APIKEY 鉴权

## 7. 构建与发布

- 使用 GitHub Actions 自动打包，构建矩阵覆盖 Windows、macOS、Linux
- 构建产物发布至 GitHub Releases
- 支持 beta 版本：以 `vX.Y.Z-beta.N` 形式的 tag 触发，并标记为 pre-release

## 8. 开发约定

1. [AGENTS.md](./AGENTS.md) 禁止修改。
2. 每完成一项功能，同步更新 [CHANGELOG.md](./CHANGELOG.md) 与本文件。
3. 注意终端 GBK 与 UTF-8 编码问题（见 3.3 节）。
4. 跨平台要求：所有功能需在 Windows、macOS、Linux 上可用；路径处理使用跨平台 API（如 `std::path::Path`），禁止硬编码路径分隔符。
5. 分支约定：日常开发在 `dev` 分支进行，功能稳定后合入 `main` 分支。
