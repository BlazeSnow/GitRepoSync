# Git Repo Sync 软件开发指南

1. 禁止修改本文件
2. 开发过程中需要处理终端GBK与UTF-8的关系
3. 更新完一项功能后，修改CHANGELOG.md和DEVELOPMENT.md

## 软件架构

1. 软件使用tauri2架构
2. 软件后端使用rust
3. 软件UI使用shadcn/ui
4. 软件支持跨平台

## 软件功能

1. 同步仓库页面：以表格形式列出仓库、源地址、目标地址
2. 提供商页面：目前支持GitHub、GitLab，用户填入对应的PAT后，列出账户及组织
3. 设置页面：修改账户密码，列出软件仓库，列出软件版本号

## Agent 使用方式

1. 软件采用MCP连接方式
2. MCP方式通过APIKEY鉴权

## 用户登录

1. 初始用户admin
2. 初始密码admin123
3. 支持持久化登录

## 发布

1. 软件使用GitHub action进行打包
2. 发布至GitHub release
3. 支持beta版本
