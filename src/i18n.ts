// 前端多语言：i18next + react-i18next。
// 中英词典拆分在 src/i18n/zh.ts 与 src/i18n/en.ts（新增键需两文件同步）；
// 浏览器语言探测（localStorage 记忆 + navigator 回退）；
// 语言切换时同步后端（set_lang 命令），保证错误提示、日志与同步结果语言一致。
import i18n, { type Resource } from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { invoke } from "@tauri-apps/api/core";
import { zh } from "./i18n/zh";
import { en } from "./i18n/en";

export type Lang = "zh" | "en";

const resources: Resource = {
  zh: { translation: zh },
  en: { translation: en },
};

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources,
    fallbackLng: "zh",
    // 先读 localStorage 记忆，再按浏览器语言；zh* 归一为 zh，其余归一为 en
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
      convertDetectedLanguage: (lng: string) =>
        lng.toLowerCase().startsWith("zh") ? "zh" : lng.toLowerCase().startsWith("en") ? "en" : lng,
    },
    interpolation: { escapeValue: false },
  });

// 语言切换时同步后端（错误提示、日志、同步结果跟随界面语言）
i18n.on("languageChanged", (lng) => {
  const lang: Lang = lng.toLowerCase().startsWith("zh") ? "zh" : "en";
  invoke("set_lang", { lang }).catch(() => {});
});

/** 切换界面语言并同步后端 */
export async function changeAppLang(lang: Lang): Promise<void> {
  await i18n.changeLanguage(lang);
}

/** 相对时间格式化（跟随界面语言） */
export function relativeTime(ms: number | null): string {
  if (ms === null || ms === undefined) return i18n.t("never");
  const diff = Date.now() - ms;
  if (diff <= 0) return i18n.t("justNow");
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return i18n.t("justNow");
  const min = Math.floor(sec / 60);
  if (min < 60) return i18n.t("minutesAgo", { count: min });
  const hour = Math.floor(min / 60);
  if (hour < 24) return i18n.t("hoursAgo", { count: hour });
  const day = Math.floor(hour / 24);
  if (day < 30) return i18n.t("daysAgo", { count: day });
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}
