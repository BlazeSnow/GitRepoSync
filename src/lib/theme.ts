// 外观主题：默认跟随系统深浅色（prefers-color-scheme），设置页可手动指定浅色/深色。
// 偏好存 localStorage（grs_theme），统一应用为 <html> 的 .dark 类；
// index.html 内联脚本在首帧前做同样的初始化，避免深色系统下启动白屏闪烁。

export type ThemePref = "system" | "light" | "dark";

const THEME_KEY = "grs_theme";

export function getThemePref(): ThemePref {
  const v = localStorage.getItem(THEME_KEY);
  return v === "light" || v === "dark" ? v : "system";
}

export function setThemePref(pref: ThemePref): void {
  localStorage.setItem(THEME_KEY, pref);
  applyTheme(pref);
}

export function applyTheme(pref: ThemePref): void {
  const dark =
    pref === "dark" ||
    (pref === "system" && window.matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.classList.toggle("dark", dark);
}

/** 跟随系统时监听系统深浅色变化，即时切换；返回取消监听函数 */
export function watchSystemTheme(): () => void {
  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  const onChange = () => {
    if (getThemePref() === "system") {
      applyTheme("system");
    }
  };
  mq.addEventListener("change", onChange);
  return () => mq.removeEventListener("change", onChange);
}
