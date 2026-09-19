# 环境准备与常用命令

> 返回 [DEVELOPMENT.md](../DEVELOPMENT.md)

## 1. 基础依赖

- **Rust**（stable，通过 [rustup](https://rustup.rs) 安装）
- **Node.js** ≥ 20 LTS
- **包管理器**：pnpm（或 npm）
- **Tauri CLI**：随前端依赖安装 `@tauri-apps/cli`，或全局安装 `cargo install tauri-cli --version "^2"`

## 2. 各平台系统依赖

### Windows

- WebView2 Runtime（Windows 10/11 一般自带）
- Visual Studio Build Tools（含 “使用 C++ 的桌面开发” 工作负载）

### macOS

- Xcode Command Line Tools（`xcode-select --install`）

### Linux（Debian/Ubuntu）

```bash
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
  libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

## 3. 终端编码（Windows 重点）

AGENTS.md 要求开发过程中处理终端 GBK 与 UTF-8 的关系：

1. 所有源码、配置与文档文件统一保存为 **UTF-8** 编码。
2. Windows 传统终端（cmd / PowerShell）默认代码页为 GBK（936），运行含中文输出的命令前先切换为 UTF-8：
   - cmd：`chcp 65001`
   - PowerShell：`[Console]::OutputEncoding = [System.Text.Encoding]::UTF8`
3. Git Bash 下通常无需额外处理；若中文显示乱码，检查 `LANG` / `LC_ALL` 是否为 `*.UTF-8`。
4. Rust 后端在 Windows 控制台打印中文日志时，注意输出编码与控制台代码页匹配，避免日志乱码。
5. 涉及仓库路径的存储与传递时，注意非 ASCII（中文）路径的编解码，统一按 UTF-8 处理。

## 4. 常用命令

推荐使用仓库根目录的 [run.ps1](../run.ps1) 快速启动（自动切 UTF-8、检查依赖、总是增量构建）：

```powershell
.\run.ps1               # 构建并后台启动 Debug 版（源码有改动时自动增量重建，脚本立即返回）
.\run.ps1 -Dev          # 开发模式（热重载，占用终端）
.\run.ps1 -Build        # 打包发布版安装包
.\run.ps1 -Rebuild      # 强制重建 Debug 版并后台启动
```

等价的原始命令：

```bash
pnpm install            # 安装前端依赖
pnpm tauri dev          # 以开发模式启动（前端热重载 + Rust 增量编译）
pnpm tauri build        # 构建发布版安装包

# 在 src-tauri 目录下：
cargo check             # 快速检查 Rust 代码
cargo fmt               # Rust 代码格式化
cargo clippy            # Rust 静态检查
```
