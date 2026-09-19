import { beforeAll, describe, expect, it } from "vitest";
// i18n.ts 在 i18next 默认单例上初始化词典，直接取同一实例控制测试语言
import i18next from "i18next";
import { relativeTime } from "./i18n";

const MIN = 60_000;
const HOUR = 3_600_000;
const DAY = 86_400_000;

// 浏览器语言探测在 jsdom 下随环境变化，显式固定为中文保证断言稳定
beforeAll(async () => {
  await i18next.changeLanguage("zh");
});

describe("relativeTime", () => {
  it("null 显示「从未」", () => {
    expect(relativeTime(null)).toBe("从未");
  });

  it("一分钟内与未来时间显示「刚刚」", () => {
    expect(relativeTime(Date.now() - 30_000)).toBe("刚刚");
    expect(relativeTime(Date.now() + 5 * MIN)).toBe("刚刚");
  });

  it("分钟 / 小时 / 天前", () => {
    expect(relativeTime(Date.now() - 5 * MIN)).toBe("5 分钟前");
    expect(relativeTime(Date.now() - 3 * HOUR)).toBe("3 小时前");
    expect(relativeTime(Date.now() - 2 * DAY)).toBe("2 天前");
  });

  it("超过 30 天显示具体日期", () => {
    const d = new Date(Date.now() - 40 * DAY);
    const p = (n: number) => String(n).padStart(2, "0");
    expect(relativeTime(d.getTime())).toBe(
      `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`,
    );
  });

  it("切换英文后文案跟随", async () => {
    await i18next.changeLanguage("en");
    expect(relativeTime(Date.now() - 5 * MIN)).toBe("5 min ago");
    await i18next.changeLanguage("zh");
  });
});
