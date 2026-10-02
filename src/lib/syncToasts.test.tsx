import { act, cleanup, renderHook, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { SyncEvent } from "./types";

// 捕获事件处理器：由测试直接触发 sync-status 回调（不依赖真实事件）
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { listen } from "@tauri-apps/api/event";
import { ToastProvider } from "@/components/Toast";
import { useSyncToasts } from "./syncToasts";

// i18n 词典初始化通常由 main.tsx 引入；组件测试需显式导入副作用模块
import "@/i18n";

afterEach(cleanup);

let emit: (payload: SyncEvent) => void = () => {};

beforeEach(async () => {
  vi.clearAllMocks();
  localStorage.clear();
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
  vi.mocked(listen).mockImplementation(async (_event, cb) => {
    emit = (payload) => (cb as (e: { payload: SyncEvent }) => void)({ payload });
    return () => {};
  });
});

it("同步成功/失败/停止事件弹出对应 toast，失败附原因；运行中不提示", async () => {
  renderHook(() => useSyncToasts(), { wrapper: ToastProvider });
  await waitFor(() => expect(listen).toHaveBeenCalled());

  // 运行中为过程态：不弹
  act(() => emit({ id: "1", name: "demo", status: "running", message: null, lastSynced: null }));
  expect(screen.queryByText("仓库「demo」同步成功")).not.toBeInTheDocument();

  act(() => emit({ id: "1", name: "demo", status: "success", message: null, lastSynced: 1 }));
  expect(screen.getByText("仓库「demo」同步成功")).toBeInTheDocument();

  // 失败 toast 附完整原因（比日志页的截断更完整）
  act(() =>
    emit({
      id: "1",
      name: "demo",
      status: "failed",
      message: "推送到 backup 失败：boom",
      lastSynced: null,
    }),
  );
  expect(screen.getByText("仓库「demo」同步失败")).toBeInTheDocument();
  expect(screen.getByText("推送到 backup 失败：boom")).toBeInTheDocument();

  act(() => emit({ id: "1", name: "demo", status: "stopped", message: null, lastSynced: null }));
  expect(screen.getByText("仓库「demo」已停止同步")).toBeInTheDocument();
});
