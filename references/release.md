# 构建与发布

> 返回 [DEVELOPMENT.md](../DEVELOPMENT.md)

## 1. 版本号规则

版本号统一维护在仓库根目录 `package.json`，其余文件由脚本同步：

| 字段 | 含义 | 示例 |
| --- | --- | --- |
| `version` | 软件版本，格式 `X.Y.Z` 或 `X.Y.Z-beta.N` | `1.0.0-beta.1` |
| `msiVersion` | Windows MSI 发布序列号，格式 `X.Y.Z`，第 N 次发布为 `1.0.N` | `1.0.1` |
| `baseVersion` | 目标版本线，格式 `vX.Y.Z`，约束 `version` 基线 | `v1.0.0` |

- `msiVersion` 独立于软件版本：MSI 版本比较只看前三段纯数字，无法表达 beta 语义；用发布次数（1.0.N）保证单调递增，旧版可直接覆盖升级
- `version.ps1` 将 `version` 同步到 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`src-tauri/Cargo.lock`，并将 `msiVersion` 写入 `tauri.conf.json` 的 `bundle.windows.wix.version`

## 2. 发布流程

```powershell
# 1. 修改 package.json 的 version / msiVersion（msiVersion 每次发布 +1）
# 2. 同步版本文件
.\version.ps1

# 3. 更新 CHANGELOG.md

# 4. 提交后打 tag（tag = v + package.json 的 version）
.\tag.ps1            # 一致性检查 + 确认 + 创建并推送 tag
.\tag.ps1 -NoPush    # 只创建本地 tag
```

- tag 推送后触发 [.github/workflows/release.yml](../.github/workflows/release.yml)，四平台矩阵（Windows x64、macOS Apple Silicon / Intel、Linux x64）打包并发布至 GitHub Releases
- `vX.Y.Z-beta.N` 形式的 tag 自动标记为 Prerelease（beta 版本）
- 工作流在打包前运行 `ci/check-version.sh` 校验版本一致性；PR 合入 `main` 时由 [.github/workflows/check.yml](../.github/workflows/check.yml) 做同样校验
- 本地发布前可手动校验：`bash ci/check-version.sh <版本号>`（如 `1.0.0-beta.1`）

## 3. Linux 打包策略

Linux 的三个产物分工如下：

| 产物      | 产出方                                            | 说明                                     |
| --------- | ------------------------------------------------- | ---------------------------------------- |
| AppImage  | Tauri（`tauri build --bundles appimage`）         | 版本号只体现在文件名，无格式限制         |
| deb / rpm | nfpm（`ci/nfpm.yaml` + `ci/linux-package.sh`）    | 版本号进入包元数据，需满足包管理器字符集 |

### 为什么 deb / rpm 不用 Tauri 自带的打包

Tauri CLI 2.12.4 起，tauri-bundler 改用 rpm crate 0.30 生成 RPM，该版本会严格校验 RPM Version 字符集（不允许 `-`）。本项目版本号形如 `1.0.0-beta.7`，含 `-`，因此 `tauri build` 在 RPM 阶段直接失败：

```text
failed to bundle project: invalid version "1.0.0-beta.7": contains invalid character (allowed: alphanumeric, '.', '_', '+', '%', '{', '}', '~', '^')
```

上游只在 Tauri v3 线修复（`to_rpm_version()` 把版本转成 `1.0.0~beta.7`），v2 线未回移。nfpm 会把 semver 拆成 Version + Prerelease，产出 `1.0.0~beta.7`，与上游 v3 行为一致。

### 版本映射

| 位置                                  | 值                                                                                                  |
| ------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `package.json` / `tauri.conf.json` / tag | `1.0.0-beta.7`（应用版本，不变）                                                                   |
| 产物文件名                            | `GitRepoSync_1.0.0-beta.7_amd64.deb`、`GitRepoSync-1.0.0-beta.7-1.x86_64.rpm`（与 Tauri 命名一致） |
| deb / rpm 包内版本字段                | `1.0.0~beta.7`（RPM 另有 Release `1`）                                                             |

> `~` 是 Debian/RPM 的预发布标记：beta 正确排在正式版 `1.0.0` 之前。历史用 Tauri 打的 deb 内部版本为 `1.0.0-beta.7`，在 Debian 里会被解析成 upstream `1.0.0` + revision `beta.7`，反而大于正式版，新方案修正了这一语义。

### CI 步骤

`release.yml` 的 Linux job 依次执行：编译二进制（`pnpm tauri build --no-bundle`）→ `bash ci/linux-package.sh` 生成 deb / rpm → `tauri-action` 以 `--bundles appimage` 打包 AppImage 并上传三个产物。

产物文件名必须与 `tauri-action` 的期望路径完全一致：该 action 按固定路径收集产物（不扫描目录），路径不一致会被静默跳过、不会上传到 Release。

### 本地打包（Linux）

Linux 上 `pnpm tauri build` 仍会在 RPM 阶段失败，请改用：

```bash
pnpm tauri build --bundles appimage   # 编译二进制 + AppImage
bash ci/linux-package.sh               # deb / rpm（脚本会自动下载并校验 nfpm）
```

### 与历史包的差异

- 显式声明了运行时依赖（Tauri 默认不声明任何 `Depends`）：deb 依赖 `libwebkit2gtk-4.1-0` 等，rpm 依赖 `webkit2gtk4.1` 等
- deb / rpm 内部版本字段由 `1.0.0-beta.7` 变为 `1.0.0~beta.7`

## 4. 本地构建

本地构建命令见 [environment.md](./environment.md) 的“常用命令”一节；完整安装包构建为 `pnpm tauri build`（产物在 `src-tauri/target/release/bundle/`）。

注意事项：

- 仓库图片使用 Git LFS，工作流 checkout 已开启 `lfs: true`
- 发布前确认 [CHANGELOG.md](../CHANGELOG.md) 已更新
- Linux 上 `pnpm tauri build` 仍会在 RPM 阶段失败（原因见上一节），请改用 `pnpm tauri build --bundles appimage` + `bash ci/linux-package.sh`
