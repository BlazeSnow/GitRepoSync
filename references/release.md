# 构建与发布

> 返回 [DEVELOPMENT.md](../DEVELOPMENT.md)

## 1. 构建发布流程

- 使用 GitHub Actions 自动打包，构建矩阵覆盖 Windows、macOS、Linux
- 构建产物发布至 GitHub Releases
- 本地构建命令见 [environment.md](./environment.md) 的“常用命令”一节

## 2. beta 版本

- 以 `vX.Y.Z-beta.N` 形式的 tag 触发
- 发布时标记为 pre-release
