import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { applyTheme, getThemePref, setThemePref, watchSystemTheme } from "./theme";

// jsdom 未实现 matchMedia（主题检测依赖），用可编程桩替代并可手动触发系统深浅色变化
let dark = false;
let listeners: ((e: MediaQueryListEvent) => void)[] = [];

function fireSystemChange(matches: boolean) {
  dark = matches;
  const event = { matches } as MediaQueryListEvent;
  [...listeners].forEach((l) => l(event));
}

beforeEach(() => {
  dark = false;
  listeners = [];
  localStorage.clear();
  document.documentElement.className = "";
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((query: string) => ({
      matches: query.includes("prefers-color-scheme: dark") && dark,
      media: query,
      addEventListener: (_: string, l: (e: MediaQueryListEvent) => void) => {
        listeners.push(l);
      },
      removeEventListener: (_: string, l: (e: MediaQueryListEvent) => void) => {
        listeners = listeners.filter((x) => x !== l);
      },
    })),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it("默认偏好为跟随系统", () => {
  expect(getThemePref()).toBe("system");
});

it("setThemePref 持久化并立即应用 .dark 类", () => {
  setThemePref("dark");
  expect(localStorage.getItem("grs_theme")).toBe("dark");
  expect(document.documentElement.classList.contains("dark")).toBe(true);

  setThemePref("light");
  expect(document.documentElement.classList.contains("dark")).toBe(false);

  setThemePref("system");
  expect(
    document.documentElement.classList.contains("dark"),
    "系统浅色",
  ).toBe(false);
});

it("applyTheme 按偏好与系统状态应用主题", () => {
  applyTheme("light");
  expect(document.documentElement.classList.contains("dark")).toBe(false);

  applyTheme("dark");
  expect(document.documentElement.classList.contains("dark")).toBe(true);

  fireSystemChange(true);
  applyTheme("system");
  expect(document.documentElement.classList.contains("dark"), "系统深色").toBe(true);

  fireSystemChange(false);
  applyTheme("system");
  expect(document.documentElement.classList.contains("dark")).toBe(false);
});

it("watchSystemTheme：跟随系统时即时切换，可取消监听", () => {
  setThemePref("system");
  const unwatch = watchSystemTheme();
  fireSystemChange(true);
  expect(document.documentElement.classList.contains("dark")).toBe(true);

  unwatch();
  fireSystemChange(false);
  expect(document.documentElement.classList.contains("dark"), "取消后不再跟随").toBe(true);
});

it("手动浅色偏好不受系统深浅色变化影响", () => {
  setThemePref("light");
  const unwatch = watchSystemTheme();
  fireSystemChange(true);
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  unwatch();
});
