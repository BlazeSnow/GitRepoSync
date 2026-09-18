# 构建与发布

> 返回 [DEVELOPMENT.md](../DEVELOPMENT.md)

## 1. 构建发布流程

- 使用 GitHub Actions 自动打包，工作流见 [.github/workflows/release.yml](../.github/workflows/release.yml)
- 构建矩阵覆盖 Windows（x86_64）、macOS（aarch64）、Linux（x86_64）
- 构建产物由 [tauri-action](https://github.com/tauri-apps/tauri-action) 发布至 GitHub Releases
- 本地构建命令见 [environment.md](./environment.md) 的“常用命令”一节

## 2. beta 版本

- 以 `vX.Y.Z-beta.N` 形式的 tag 触发（如 `v0.2.0-beta.1`），工作流自动将发布标记为 pre-release
- 正式版本使用 `vX.Y.Z` 形式的 tag

## 3. 发布操作

```bash
# 正式发布
git tag v0.2.0
git push origin v0.2.0

# beta 发布
git tag v0.3.0-beta.1
git push origin v0.3.0-beta.1
```

注意事项：

- tag 中的版本号需与 `src-tauri/tauri.conf.json` 的 `version` 一致（建议发版前同步修改）
- 仓库图片使用 Git LFS，工作流中的 checkout 已开启 `lfs: true`
- 发布前确认 [CHANGELOG.md](../CHANGELOG.md) 已更新
