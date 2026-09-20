import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { OperationLog } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: { listLogs: vi.fn() },
}));

import { api } from "@/lib/api";
import { LogsPage } from "./LogsPage";

afterEach(cleanup);

beforeEach(async () => {
  vi.clearAllMocks();
  // 先 import @/i18n 触发实例初始化（资源注册），再切语言
  await import("@/i18n");
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
});

function log(id: number, partial: Partial<OperationLog> = {}): OperationLog {
  return { id, action: `操作 ${id}`, operator: "admin", createdAt: 0, ...partial };
}

it("加载并渲染日志行：时间格式化、操作、操作人；空列表显示空态", async () => {
  vi.mocked(api.listLogs)
    .mockResolvedValueOnce([
      log(1, { createdAt: new Date(2026, 8, 20, 8, 5, 9).getTime() }),
      log(2, { operator: "mcp", action: "Agent 添加仓库" }),
    ])
    .mockResolvedValueOnce([]);

  const { unmount } = render(<LogsPage token="tok" />);
  expect(await screen.findByText("操作 1")).toBeInTheDocument();
  expect(screen.getByText("2026-09-20 08:05:09")).toBeInTheDocument();
  expect(screen.getByText("mcp")).toBeInTheDocument();

  // 点击刷新后列表为空，显示空态
  fireEvent.click(screen.getByRole("button", { name: "刷新" }));
  expect(await screen.findByText("暂无操作记录")).toBeInTheDocument();
  unmount();
});

it("加载失败显示错误信息", async () => {
  vi.mocked(api.listLogs).mockRejectedValue("会话已过期");

  render(<LogsPage token="tok" />);
  expect(await screen.findByText("会话已过期")).toBeInTheDocument();
});
