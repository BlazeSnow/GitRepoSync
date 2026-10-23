//! 帮助文本：CLI `--help` 与 MCP `help` 方法共用同一份内容，避免两处描述漂移。

/// 帮助文本（CLI `--help` 与 MCP `help` 方法共用）
pub fn help_text() -> String {
    format!(
        "Git Repo Sync {ver} - 跨平台 Git 仓库同步工具\n\n\
用法:\n  \
git-repo-sync [命令] [参数]\n\n\
命令:\n  \
(无命令)  启动图形界面\n  \
mcp       以 stdio 模式运行 MCP 服务（供 Agent 客户端连接）\n\n\
参数:\n  \
mcp --api-key <KEY>   MCP APIKEY（优先级高于环境变量）\n  \
--help, -h            显示本帮助\n  \
--version, -V         显示版本号\n\n\
环境变量:\n  \
GIT_REPO_SYNC_API_KEY=<KEY>   MCP APIKEY 鉴权\n  \
GIT_REPO_SYNC_LANG=<zh|en>    MCP 消息语言（默认 zh）\n\n\
仓库来源:\n  \
软件不代管仓库克隆与远端。请自行把仓库克隆到本地基地址下的目录（默认 ~/repo，\
可在设置页修改），并用 git remote 配置远端：origin 为源地址，其余远端为备份目标\
（upstream 为保留名，不参与同步）。软件实时扫描基地址得到仓库列表，目录消失即不再同步。\n\n\
MCP 工具（8 个）:\n  \
list_repos      列出基地址内的仓库及最近同步状态\n  \
sync_repo       立即同步指定仓库（参数 name）\n  \
sync_repos      批量同步（参数 names 列表，或 days 范围）\n  \
stop_syncs      停止同步（参数 names 可选，缺省全部活动任务）\n  \
get_sync_status 查询所有仓库最近同步状态\n  \
list_logs       按时间倒序列出操作日志（参数 limit 可选）\n  \
get_base_dir    查询本地仓库基地址\n  \
set_base_dir    修改本地仓库基地址\n\n\
示例:\n  \
git-repo-sync mcp --api-key grs_xxx\n  \
git-repo-sync --version",
        ver = env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 帮助文本覆盖当前软件现状：命令、环境变量、MCP 工具清单与参数口径
    #[test]
    fn help_text_reflects_current_state() {
        let t = help_text();
        assert!(t.contains(&format!("Git Repo Sync {}", env!("CARGO_PKG_VERSION"))));
        // 命令与参数
        assert!(t.contains("mcp"));
        assert!(t.contains("--api-key"));
        assert!(t.contains("--version"));
        assert!(t.contains("GIT_REPO_SYNC_API_KEY"));
        assert!(t.contains("GIT_REPO_SYNC_LANG"));
        // 仓库来源：软件不代管克隆与远端
        assert!(t.contains("不代管仓库克隆与远端"));
        assert!(t.contains("实时扫描"));
        // MCP 工具清单：8 个，与 tools/list 一致
        for tool in [
            "list_repos",
            "sync_repo",
            "sync_repos",
            "stop_syncs",
            "get_sync_status",
            "list_logs",
            "get_base_dir",
            "set_base_dir",
        ] {
            assert!(t.contains(tool), "帮助文本应列出工具 {tool}");
        }
        assert!(t.contains("MCP 工具（8 个）"));
        // 已移除的工具不应再出现在帮助里
        for gone in ["add_repo", "update_repo", "remove_repo", "update_repos", "discover_repos"] {
            assert!(!t.contains(gone), "已移除的 {gone} 不应出现在帮助文本");
        }
    }
}
