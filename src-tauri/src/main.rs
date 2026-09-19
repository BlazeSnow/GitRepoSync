#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// 编译期内嵌 locales/ 语言包为静态 LOCALES（回落 zh-CN）
fluent_i18n::i18n!("locales", fallback = "zh-CN");

mod auth;
mod discover;
mod git;
mod lang;
mod mcp;
mod repos;
mod settings;
mod state;
mod sync;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // 命令行帮助与版本（供 shell / Agent 使用）：输出后立即退出，不启动 GUI
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        ensure_console_for_cli();
        print_help();
        std::process::exit(0);
    }
    if args
        .iter()
        .any(|a| a == "--version" || a == "-V" || a == "version")
    {
        ensure_console_for_cli();
        println!("git-repo-sync {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // mcp 子命令：以 stdio 模式运行 MCP 服务（供 Agent 客户端作为子进程拉起）
    if args.first().map(String::as_str) == Some("mcp") {
        run_mcp_stdio(&args);
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("解析数据目录失败: {e}"))?;
            let state = Arc::new(AppState::open(data_dir.join("app.db"))?);
            state.reset_running_repos();
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth::login,
            auth::restore_session,
            auth::logout,
            auth::change_password,
            repos::list_repos,
            repos::discover_repos,
            repos::save_repo,
            repos::delete_repo,
            sync::start_sync,
            sync::stop_sync,
            settings::get_app_info,
            settings::get_base_dir,
            settings::set_base_dir,
            settings::get_mcp_config,
            settings::regenerate_mcp_api_key,
            settings::list_operation_logs,
            lang::set_lang,
        ])
        .run(tauri::generate_context!())
        .expect("Git Repo Sync 启动失败");
}

/// Windows 下 release 构建为 GUI 子系统（无控制台）：
/// - 被 Agent 以管道/重定向捕获 stdout 时，标准句柄已有效，直接输出即可
/// - 在交互 shell 中直接运行时附加父控制台并重新绑定标准句柄，使输出可见
// 非 Windows 平台：终端子系统的进程天然有控制台，stdout 直接可用，无需附加
#[cfg(not(windows))]
fn ensure_console_for_cli() {}

#[cfg(windows)]
fn ensure_console_for_cli() {
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    unsafe {
        let out = GetStdHandle(STD_OUTPUT_HANDLE);
        if !out.is_null() && out != INVALID_HANDLE_VALUE {
            return; // stdout 已有效（管道 / 重定向 / 已附加控制台）
        }
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // 无父控制台（如双击启动），静默放弃
        }
        let conout: Vec<u16> = "CONOUT\0".encode_utf16().collect();
        let handle = CreateFileW(
            conout.as_ptr(),
            GENERIC_WRITE,
            FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        SetStdHandle(STD_OUTPUT_HANDLE, handle);
        SetStdHandle(STD_ERROR_HANDLE, handle);
    }
}

fn print_help() {
    println!(
        "Git Repo Sync {} - 跨平台 Git 仓库同步工具\n\n\
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
示例:\n  \
git-repo-sync mcp --api-key grs_xxx\n  \
git-repo-sync --version",
        env!("CARGO_PKG_VERSION")
    );
    use std::io::Write;
    let _ = std::io::stdout().flush();
}

fn run_mcp_stdio(args: &[String]) {
    // APIKEY：优先 --api-key 参数，其次环境变量 GIT_REPO_SYNC_API_KEY
    let mut provided = std::env::var("GIT_REPO_SYNC_API_KEY").ok();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--api-key" {
            if let Some(v) = args.get(i + 1) {
                provided = Some(v.clone());
            }
            break;
        }
        i += 1;
    }

    // 与 Tauri 运行时的 app_data_dir 保持一致：系统数据目录 + 应用标识符
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            eprintln!("Git Repo Sync MCP 启动失败：无法定位系统数据目录");
            std::process::exit(1);
        })
        .join("com.blazesnow.gitreposync");
    // 启动失败不 panic：stderr 说明原因后以非零码退出，客户端可见（GUI 进程写库忙等场景）
    let state = match AppState::open(data_dir.join("app.db")) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("Git Repo Sync MCP 启动失败：{e}");
            std::process::exit(1);
        }
    };
    mcp::run_stdio(state.clone(), provided);
    // stdin 已关闭（客户端断开）：等待在途同步完成后再退出，避免中断同步
    state.wait_syncs_idle();
}
