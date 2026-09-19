//! 后端多语言。文案集中在 `locales/zh-CN/main.ftl`（默认与回落语言）与
//! `locales/en-US/main.ftl`，编译期内嵌为静态 `LOCALES`（main.rs 的 `i18n!` 宏）。
//! - GUI：语言由前端通过 `set_lang` 命令设置（存进程内全局，未设置时取环境变量
//!   `GIT_REPO_SYNC_LANG`，仍缺省为中文）
//! - MCP：会话语言取 initialize 请求的 `locale` 字段，环境变量优先；工具名称为
//!   协议契约不翻译
//! - 查询始终显式传入语言（`LOCALES.lookup(&lang.id(), key)`），不使用
//!   fluent-i18n 的线程局部 set_locale，后台同步线程不会串语言

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::RwLock;

use fluent_i18n::fluent_templates::{langid, LanguageIdentifier, Loader};
use fluent_i18n::FluentValue;

/// 后端消息语言
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Lang {
    #[default]
    Zh,
    En,
}

static LANG_ZH: LanguageIdentifier = langid!("zh-CN");
static LANG_EN: LanguageIdentifier = langid!("en-US");

impl Lang {
    fn id(self) -> &'static LanguageIdentifier {
        match self {
            Self::Zh => &LANG_ZH,
            Self::En => &LANG_EN,
        }
    }

    /// 解析单个语言标签（MCP initialize 的 locale / GIT_REPO_SYNC_LANG 环境变量）：
    /// `zh*` 为中文，其他非空值视为英文；空值返回 None 由调用方决定默认
    pub fn parse_tag(tag: Option<&str>) -> Option<Lang> {
        let tag = tag?.trim();
        if tag.is_empty() {
            return None;
        }
        Some(if tag.to_ascii_lowercase().starts_with("zh") {
            Lang::Zh
        } else {
            Lang::En
        })
    }

    /// 进程初始语言：GIT_REPO_SYNC_LANG 环境变量优先，缺省中文
    pub fn from_env() -> Lang {
        Self::parse_tag(std::env::var("GIT_REPO_SYNC_LANG").ok().as_deref()).unwrap_or(Lang::Zh)
    }

    /// 列表连接符：中文用顿号式分号，英文用分号
    pub fn sep(self) -> &'static str {
        match self {
            Self::Zh => "；",
            Self::En => "; ",
        }
    }
}

static GUI_LANG: RwLock<Option<Lang>> = RwLock::new(None);

/// GUI 模式当前语言（前端 set_lang 设置）
pub fn gui_lang() -> Lang {
    let guard = GUI_LANG
        .read()
        .unwrap_or_else(|p| p.into_inner());
    guard.unwrap_or_else(Lang::from_env)
}

pub fn set_gui_lang(lang: Lang) {
    *GUI_LANG.write().unwrap_or_else(|p| p.into_inner()) = Some(lang);
}

/// 前端设置界面语言（登录前后均可调用；影响后端消息与日志语言）
#[tauri::command]
pub fn set_lang(lang: String) -> Result<(), String> {
    match Lang::parse_tag(Some(&lang)) {
        Some(l) => {
            set_gui_lang(l);
            Ok(())
        }
        None => Err(format!("unsupported lang: {lang}")),
    }
}

/// 按语言查静态文案；缺失键回落 zh-CN
pub fn tr(lang: Lang, key: &str) -> String {
    crate::LOCALES.lookup(&lang.id(), key)
}

/// 按语言查带参文案（字符串参数）
pub fn tr_a(lang: Lang, key: &str, args: &[(&str, &str)]) -> String {
    // 查询接口要求 'static 的参数表，此处按需拷贝（消息量小，开销可忽略）
    let args: HashMap<Cow<'static, str>, FluentValue<'static>> = args
        .iter()
        .map(|(k, v)| (Cow::Owned(k.to_string()), FluentValue::from(v.to_string())))
        .collect();
    crate::LOCALES.lookup_with_args(&lang.id(), key, &args)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 语言标签解析：zh* 归中文，其余非空值归英文，空值交调用方定默认
    #[test]
    fn parse_tag_maps_locales() {
        assert_eq!(Lang::parse_tag(Some("zh-CN")), Some(Lang::Zh));
        assert_eq!(Lang::parse_tag(Some("zh")), Some(Lang::Zh));
        assert_eq!(Lang::parse_tag(Some("en-US")), Some(Lang::En));
        assert_eq!(Lang::parse_tag(Some(" fr ")), Some(Lang::En), "非 zh 一律视为英文");
        assert_eq!(Lang::parse_tag(Some("")), None);
        assert_eq!(Lang::parse_tag(Some("   ")), None);
        assert_eq!(Lang::parse_tag(None), None);
    }

    #[test]
    fn sep_follows_language() {
        assert_eq!(Lang::Zh.sep(), "；");
        assert_eq!(Lang::En.sep(), "; ");
    }

    /// 文案查询：分语言取值、参数替换、缺失键双语行为一致
    #[test]
    fn tr_lookups_params_and_fallback() {
        assert_eq!(tr(Lang::Zh, "repo-not-found"), "仓库不存在");
        assert_eq!(tr(Lang::En, "repo-not-found"), "Repository not found");
        assert!(
            tr_a(Lang::Zh, "log-repo-added", &[("name", "demo")]).contains("demo"),
            "参数被替换进文案"
        );
        // 双语言包均缺失的键：En 回落 zh-CN，两者行为一致
        assert_eq!(tr(Lang::En, "no-such-key"), tr(Lang::Zh, "no-such-key"));
    }
}
