import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
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
import { SyncPage, filterStale } from "./SyncPage";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

beforeAll(() => {
  // Radix Select 依赖的浏览器 API 在 jsdom 中缺失：滚动、尺寸观察与指针捕获打桩
  Element.prototype.scrollIntoView = vi.fn() as unknown as typeof Element.prototype.scrollIntoView;
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  class ResizeObserverStub {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = ResizeObserverStub;
});

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

const backupTarget: Repo["targets"][number] = {
  remote: "backup",
  url: "https://gitlab.com/u/x.git",
  lastStatus: "idle",
  lastMessage: null,
  lastSynced: null,
};

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

it("无同步运行时仅显示「开始同步」，停止按钮不出现", async () => {
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
  expect(screen.queryByRole("button", { name: "开始同步" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "停止同步" })).not.toBeInTheDocument();
});

it("有仓库同步中时仅显示「停止同步」，点击终止全部同步", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({ id: "r1", name: "busy", lastStatus: "running", targets: [backupTarget] }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);
  vi.mocked(api.stopSync).mockResolvedValue(undefined);

  render(<SyncPage token="tok" />);
  expect(await screen.findByRole("button", { name: "停止同步" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /开始同步/ })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "停止同步" }));
  await waitFor(() => expect(api.stopSync).toHaveBeenCalledWith("tok", null));
});

it("点击「刷新仓库」重新扫描加载，MCP 新增的仓库可见", async () => {
  vi.mocked(api.discoverRepos)
    .mockResolvedValueOnce([])
    .mockResolvedValueOnce([repo({ id: "mcp", name: "mcp-added", targets: [backupTarget] })]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  render(<SyncPage token="tok" />);
  await screen.findByRole("combobox");
  expect(screen.queryByText("mcp-added")).not.toBeInTheDocument();
  expect(api.discoverRepos).toHaveBeenCalledTimes(1);

  fireEvent.click(screen.getByRole("button", { name: "刷新仓库" }));
  expect(await screen.findByText("mcp-added")).toBeInTheDocument();
  expect(api.discoverRepos).toHaveBeenCalledTimes(2);
});

it("编辑弹窗内删除仓库：右键编辑 → 删除仓库 → 确认后调用 deleteRepo", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([repo({ targets: [backupTarget] })]);
  vi.mocked(api.listRepos).mockResolvedValue([]);
  vi.mocked(api.deleteRepo).mockResolvedValue(undefined);

  render(<SyncPage token="tok" />);

  // 右键行打开菜单，进入编辑弹窗
  fireEvent.contextMenu(await screen.findByText("demo"));
  fireEvent.click(screen.getByRole("button", { name: "编辑仓库" }));

  // 弹窗内删除 → 确认弹窗 → 确认
  fireEvent.click(await screen.findByRole("button", { name: "删除仓库" }));
  expect(await screen.findByText(/确定要删除仓库/)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "删除" }));

  await waitFor(() => expect(api.deleteRepo).toHaveBeenCalledWith("tok", "id-1"));
});

it("filterStale：all 显示全部；N 天范围仅保留已配置且超期或从未同步的仓库", () => {
  const now = Date.now();
  const day = 86_400_000;
  const repos = [
    repo({ id: "fresh", name: "fresh", lastSynced: now, targets: [backupTarget] }),
    repo({ id: "stale", name: "stale", lastSynced: now - 2 * day, targets: [backupTarget] }),
    repo({ id: "never", name: "never", lastSynced: null, targets: [backupTarget] }),
    repo({ id: "unconf", name: "unconf", source: "", targets: [] }),
  ];
  expect(filterStale(repos, "all")).toHaveLength(4);
  expect(filterStale(repos, "1").map((r) => r.id)).toEqual(["stale", "never"]);
  // 30 天口径下 2 天前同步过的仓库已足够「新鲜」，只剩从未同步的
  expect(filterStale(repos, "30").map((r) => r.id)).toEqual(["never"]);
});

it("范围下拉选择 N 天后，表格仅显示符合范围的仓库且按钮计数一致", async () => {
  const now = Date.now();
  const day = 86_400_000;
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({ id: "fresh", name: "fresh", lastSynced: now, targets: [backupTarget] }),
    repo({ id: "stale", name: "stale", lastSynced: now - 2 * day, targets: [backupTarget] }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  render(<SyncPage token="tok" />);
  expect(await screen.findByText("fresh")).toBeInTheDocument();
  expect(screen.getByText("stale")).toBeInTheDocument();

  // jsdom 下 Radix trigger 的指针类型初始为 touch：click 即打开下拉（确定性路径）
  fireEvent.click(screen.getByRole("combobox"));
  // 选项选中：Radix 的 onClick 路径（指针类型非 mouse）直接触发选中
  const option = await screen.findByRole("option", { name: "1 天内未同步" });
  fireEvent.click(option);

  // 表格只剩超期的 stale，按钮计数同步为 1
  await waitFor(() => expect(screen.queryByText("fresh")).not.toBeInTheDocument());
  expect(screen.getByText("stale")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "开始同步（1 个）" })).toBeInTheDocument();
});
