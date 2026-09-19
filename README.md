# Git Repo Sync

跨平台 Git 仓库同步桌面工具：把本地基地址作为中转站，自动将仓库从源远端（origin）备份到你配置的多个目标远端——一次同步，多端备份。

支持 Windows、macOS（Apple Silicon / Intel）与 Linux。

## 下载

前往 [Releases](https://github.com/BlazeSnow/GitRepoSync/releases/latest) 下载对应平台的安装包：

| 平台                | 安装包                                       |
| ------------------- | -------------------------------------------- |
| Windows x64         | `GitRepoSync_<版本>_x64-setup.exe` 或 `.msi` |
| macOS Apple Silicon | `GitRepoSync_<版本>_aarch64.dmg`             |
| macOS Intel         | `GitRepoSync_<版本>_x64.dmg`                 |
| Linux x64           | `.deb` / `.rpm` / `.AppImage`                |

`vX.Y.Z-beta.N` 形式的版本为测试版（beta），功能更新更频繁，但可能存在不稳定因素。

## 快速上手

1. **登录**：初始账号 `admin`，初始密码 `admin123`（首次启动自动创建），可勾选「保持登录 30 天」；密码可在设置页修改
2. **设置基地址**：在设置页通过系统目录选择器指定本地中转目录（默认为本机用户目录下的 `repo` 文件夹）
3. **放入仓库**：把要备份的仓库 clone 或移动到基地址目录下，软件会自动发现并登记——`origin` 远端作为源地址，**其余全部远端自动登记为备份目标**
4. **一键同步**：在同步仓库页选择同步范围（全部 / 1 / 3 / 7 / 30 天内未同步），点击「开始同步」

也可以在「添加仓库」中手动录入仓库与备份目标，不依赖基地址内的目录。

## 工作原理

每次同步执行三步流水线（调用系统 git）：

1. **拉取**：从源仓库 fetch 最新状态到基地址中转目录（不合并工作区，不受本地未提交改动影响）
2. **更新**：拉取 LFS 文件（需系统安装 [git-lfs](https://git-lfs.com)）与 submodule
3. **推送**：把全部分支与标签推送到每个备份远端，并与之强制对齐（`--prune`）

多个备份目标依次推送，单个目标失败不影响其余目标；每个目标的状态独立记录。

## 功能一览

- 🔄 **1 对多备份**：origin 为主源，其余远端（gitee / gitlab / 自命名均可）全部作为备份目标
- 🔍 **自动发现**：基地址内的仓库自动登记，新增 / 删除远端自动同步到备份列表
- 🕒 **定时提醒式同步**：按「最近同步时间」圈定范围，一键补齐长期未备份的仓库
- 📦 **LFS 与 submodule 支持**：完整备份大文件与子模块（未安装 git-lfs 时自动降级并提示）
- 📜 **操作日志**：登录、同步、配置变更等全部操作入库 SQLite，按时间倒序可查
- 🌐 **中英双语**：设置页可切换界面语言
- 🌗 **深色模式**：跟随系统深浅色自动切换，也可在设置页手动指定浅色 / 深色
- 🤖 **Agent 接入**：内置 MCP 服务，Agent 可直接管理仓库并触发同步

## Agent（MCP）接入

软件通过 MCP stdio 方式供 Agent 连接，使用 APIKEY 鉴权。API Key 在设置页或 MCP 页复制：

1. 在设置页重新生成并复制 API Key
2. 在 MCP 客户端配置中添加（路径按平台调整）：

```json
{
  "mcpServers": {
    "git-repo-sync": {
      "command": "C:\\Program Files\\GitRepoSync\\git-repo-sync.exe",
      "args": ["mcp"],
      "env": { "GIT_REPO_SYNC_API_KEY": "<你的 API Key>" }
    }
  }
}
```

可用工具：`list_repos`、`discover_repos`、`add_repo`、`update_repo`、`remove_repo`、`sync_repo`、`get_sync_status`、`list_logs`、`get_base_dir`、`set_base_dir`。其中 `add_repo` 按仓库名幂等（同名仓库已存在时合并目标而非新建），`remove_repo` 为软删除（隐藏条目并清空目标，不影响基地址内的目录），`list_logs` 可查询全部操作历史。

## 命令行

可执行文件自带命令行接口（不启动窗口）：

```text
git-repo-sync --help      查看全部命令与参数
git-repo-sync --version   查看版本号
git-repo-sync mcp         以 stdio 模式运行 MCP 服务
```

## 常见问题

- **提示 git 命令失败？** 确认系统已安装 Git 并加入 PATH；私有仓库需提前配置好凭据（HTTPS 凭据管理器或 SSH 免密）
- **LFS 文件没有备份？** 安装 [git-lfs](https://git-lfs.com) 后重新同步；备份目标为 GitLab 且走 SSH 时 LFS 对象不会上传，请改用 HTTPS 地址
- **仓库显示「未配置」？** 该仓库缺少 origin 源地址或没有任何备份目标远端，配置后即可参与同步

## 许可证

[AGPL-3.0](./LICENSE)
