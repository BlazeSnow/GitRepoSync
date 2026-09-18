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

## 3. 本地构建

本地构建命令见 [environment.md](./environment.md) 的“常用命令”一节；完整安装包构建为 `pnpm tauri build`（产物在 `src-tauri/target/release/bundle/`）。

注意事项：

- 仓库图片使用 Git LFS，工作流 checkout 已开启 `lfs: true`
- 发布前确认 [CHANGELOG.md](../CHANGELOG.md) 已更新
