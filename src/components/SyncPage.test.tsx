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
import { SyncPage, filterStale, sortRepos, toggleSort, type SortSpec } from "./SyncPage";

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
  // 范围选择持久化到 localStorage：每个用例从干净状态开始
  localStorage.clear();
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

it("右键表格行弹出菜单：立即同步、连续右键换行切换、点击外部关闭", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({ id: "a", name: "alpha", targets: [backupTarget] }),
    repo({ id: "b", name: "beta", targets: [backupTarget] }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);
  vi.mocked(api.startSync).mockResolvedValue(1);

  render(<SyncPage token="tok" />);
  const alpha = await screen.findByText("alpha");

  // 右键行 → 菜单出现（编辑 / 开始同步 / 删除）
  fireEvent.contextMenu(alpha);
  expect(screen.getByRole("button", { name: "编辑仓库" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "删除" })).toBeInTheDocument();

  // 菜单中的「开始同步」（与工具栏按钮同名，取最后一个）针对该行发起同步
  const syncButtons = screen.getAllByRole("button", { name: "开始同步" });
  fireEvent.click(syncButtons[syncButtons.length - 1]);
  await waitFor(() => expect(api.startSync).toHaveBeenCalledWith("tok", ["a"]));

  // 菜单已随点击关闭
  expect(screen.queryByRole("button", { name: "编辑仓库" })).not.toBeInTheDocument();

  // 再次右键 alpha 打开菜单，随后直接右键 beta：菜单切换到 beta 而非消失
  // （回归：openMenu 阻止冒泡，window 的关闭监听不得清空新菜单）
  fireEvent.contextMenu(screen.getByText("alpha"));
  expect(screen.getByRole("button", { name: "编辑仓库" })).toBeInTheDocument();
  fireEvent.contextMenu(screen.getByText("beta"));
  fireEvent.click(screen.getByRole("button", { name: "编辑仓库" }));
  expect(await screen.findByLabelText("仓库名称")).toHaveValue("beta");
  fireEvent.click(screen.getByRole("button", { name: "取消" }));

  // 左键点击菜单外关闭
  fireEvent.contextMenu(screen.getByText("alpha"));
  expect(screen.getByRole("button", { name: "编辑仓库" })).toBeInTheDocument();
  fireEvent.click(document.body);
  expect(screen.queryByRole("button", { name: "编辑仓库" })).not.toBeInTheDocument();
});

it("表头/空白区右键仅阻止默认行为，不弹出菜单", async () => {
  vi.mocked(api.discoverRepos).mockResolvedValue([repo({ targets: [backupTarget] })]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  const { container } = render(<SyncPage token="tok" />);
  await screen.findByText("demo");

  // 表头右键：容器 handler 阻止原生菜单，且无自定义菜单出现
  fireEvent.contextMenu(screen.getByText("仓库"));
  expect(screen.queryByRole("button", { name: "编辑仓库" })).not.toBeInTheDocument();
  expect(container.querySelector(".bg-popover")).toBeNull();
});

it("切页（卸载）后重进保持同步范围选择，表格与计数随之恢复", async () => {
  const now = Date.now();
  const day = 86_400_000;
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({ id: "fresh", name: "fresh", lastSynced: now, targets: [backupTarget] }),
    repo({ id: "stale", name: "stale", lastSynced: now - 2 * day, targets: [backupTarget] }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  // 第一次进入：选择「1 天内未同步」（选择写入 localStorage）
  const first = render(<SyncPage token="tok" />);
  fireEvent.click(await screen.findByRole("combobox"));
  fireEvent.click(await screen.findByRole("option", { name: "1 天内未同步" }));
  await waitFor(() => expect(screen.queryByText("fresh")).not.toBeInTheDocument());
  first.unmount();

  // 第二次进入（模拟从日志页切回）：范围不重置为全部
  render(<SyncPage token="tok" />);
  await screen.findByRole("combobox");
  expect(screen.getByRole("combobox")).toHaveTextContent("1 天内未同步");
  expect(screen.queryByText("fresh")).not.toBeInTheDocument();
  expect(screen.getByText("stale")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "开始同步（1 个）" })).toBeInTheDocument();
});

it("sortRepos：状态问题优先且未配置最后；时间从未同步最先；toggleSort 三态循环", () => {
  const now = Date.now();
  const day = 86_400_000;
  const repos = [
    repo({ id: "1", name: "a", lastStatus: "idle", lastSynced: now, targets: [backupTarget] }),
    repo({ id: "2", name: "b", lastStatus: "failed", lastSynced: now - day, targets: [backupTarget] }),
    repo({ id: "3", name: "c", lastStatus: "success", lastSynced: null, targets: [backupTarget] }),
    repo({ id: "4", name: "d", lastStatus: "success", lastSynced: now, source: "", targets: [] }),
  ];
  const names = (rows: Repo[]) => rows.map((r) => r.name);

  // 默认（null）保持名称序
  const def: SortSpec = { key: null, dir: "asc" };
  expect(names(sortRepos(repos, def))).toEqual(["a", "b", "c", "d"]);
  // 状态升序：失败 > 未同步 > 成功，未配置最后；降序相反
  expect(names(sortRepos(repos, { key: "status", dir: "asc" }))).toEqual(["b", "a", "c", "d"]);
  expect(names(sortRepos(repos, { key: "status", dir: "desc" }))).toEqual(["c", "a", "b", "d"]);
  // 时间升序：从未同步（null）最先，其后从旧到新；降序相反
  expect(names(sortRepos(repos, { key: "lastSynced", dir: "asc" }))).toEqual(["c", "b", "a", "d"]);
  expect(names(sortRepos(repos, { key: "lastSynced", dir: "desc" }))).toEqual(["a", "b", "c", "d"]);

  // 三态循环：未排 → 升 → 降 → 恢复默认
  expect(toggleSort(def, "status")).toEqual({ key: "status", dir: "asc" });
  expect(toggleSort({ key: "status", dir: "asc" }, "status")).toEqual({ key: "status", dir: "desc" });
  expect(toggleSort({ key: "status", dir: "desc" }, "status")).toEqual(def);
  // 换列直接从升序开始
  expect(toggleSort({ key: "status", dir: "desc" }, "lastSynced")).toEqual({
    key: "lastSynced",
    dir: "asc",
  });
});

it("点击状态/时间表头切换排序：升 → 降 → 恢复默认名称序", async () => {
  const now = Date.now();
  const day = 86_400_000;
  vi.mocked(api.discoverRepos).mockResolvedValue([
    repo({ id: "1", name: "alpha", lastStatus: "idle", lastSynced: now, targets: [backupTarget] }),
    repo({
      id: "2",
      name: "beta",
      lastStatus: "failed",
      lastMessage: "boom",
      lastSynced: now - day,
      targets: [backupTarget],
    }),
    repo({ id: "3", name: "gamma", lastStatus: "success", lastSynced: null, targets: [backupTarget] }),
  ]);
  vi.mocked(api.listRepos).mockResolvedValue([]);

  render(<SyncPage token="tok" />);
  await screen.findByText("alpha");
  // 表体行首列为仓库名
  const names = () =>
    screen
      .getAllByRole("row")
      .slice(1)
      .map((r) => r.querySelector("td")?.textContent);
  expect(names()).toEqual(["alpha", "beta", "gamma"]);

  // 状态表头：升序（失败在前）带 ↑，再点降序，三点恢复默认
  const statusHeader = screen.getByRole("columnheader", { name: /状态/ });
  fireEvent.click(statusHeader);
  expect(names()).toEqual(["beta", "alpha", "gamma"]);
  expect(screen.getByRole("columnheader", { name: /状态/ })).toHaveTextContent("↑");
  fireEvent.click(screen.getByRole("columnheader", { name: /状态/ }));
  expect(names()).toEqual(["gamma", "alpha", "beta"]);
  expect(screen.getByRole("columnheader", { name: /状态/ })).toHaveTextContent("↓");
  fireEvent.click(screen.getByRole("columnheader", { name: /状态/ }));
  expect(names()).toEqual(["alpha", "beta", "gamma"]);
  expect(screen.getByRole("columnheader", { name: /状态/ })).not.toHaveTextContent("↑");

  // 时间表头：升序从未同步在前，降序最新在前
  fireEvent.click(screen.getByRole("columnheader", { name: /上次同步/ }));
  expect(names()).toEqual(["gamma", "beta", "alpha"]);
  fireEvent.click(screen.getByRole("columnheader", { name: /上次同步/ }));
  expect(names()).toEqual(["alpha", "beta", "gamma"]);
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
