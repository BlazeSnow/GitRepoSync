import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Repo } from "@/lib/types";

// api 与 Tauri 事件打桩：组件测试不触碰后端
vi.mock("@/lib/api", () => ({
  api: {
    discoverRepos: vi.fn(),
    listRepos: vi.fn(),
    startSync: vi.fn(),
    stopSync: vi.fn(),
    saveRepo: vi.fn(),
    deleteRepo: vi.fn(),
  },
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

import { api } from "@/lib/api";
import { SyncPage } from "./SyncPage";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

function repo(partial: Partial<Repo>): Repo {
  return {
    id: "id-1",
    name: "demo",
    source: "https://github.com/u/demo.git",
    lastSynced: null,
    lastStatus: "idle",
    lastMessage: null,
    targets: [],
    ...partial,
  };
}

beforeEach(async () => {
  vi.clearAllMocks();
  // 语言固定中文，断言界面文案
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
});

it("渲染仓库表格：地址列含源与目标，未配置仓库有标记", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({
      targets: [
        {
          remote: "backup",
          url: "https://gitlab.com/u/demo.git",
          lastStatus: "idle",
          lastMessage: null,
          lastSynced: null,
        },
      ],
    }),
    repo({ id: "id-2", name: "bare", source: "", targets: [] }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  render(<SyncPage token="tok" />);

  expect(await screen.findByText("demo")).toBeInTheDocument();
  expect(screen.getByText(/origin: https:\/\/github\.com\/u\/demo\.git/)).toBeInTheDocument();
  expect(screen.getByText(/backup: https:\/\/gitlab\.com\/u\/demo\.git/)).toBeInTheDocument();
  // 未配置（无源、无目标）仓库显示标记
  expect(screen.getByText("bare")).toBeInTheDocument();
  expect(screen.getAllByText("未配置").length).toBeGreaterThan(0);
});

it("全部配置齐全时「开始同步」可用，点击后按范围发起同步", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({
      targets: [
        {
          remote: "backup",
          url: "https://gitlab.com/u/demo.git",
          lastStatus: "idle",
          lastMessage: null,
          lastSynced: null,
        },
      ],
    }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);
  vi.mocked(api.startSync).mockResolvedValue(1);

  render(<SyncPage token="tok" />);
  const button = await screen.findByRole("button", { name: "开始同步" });
  await waitFor(() => expect(button).toBeEnabled());
  fireEvent.click(button);

  await waitFor(() => expect(api.startSync).toHaveBeenCalled());
  expect(api.startSync).toHaveBeenCalledWith("tok", ["id-1"]);
});

it("配置仓库从未同步且「停止同步」在无运行任务时禁用", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({
      targets: [
        {
          remote: "backup",
          url: "https://gitlab.com/u/demo.git",
          lastStatus: "idle",
          lastMessage: null,
          lastSynced: null,
        },
      ],
    }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  render(<SyncPage token="tok" />);
  expect(await screen.findByText("从未")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "停止同步" })).toBeDisabled();
});
