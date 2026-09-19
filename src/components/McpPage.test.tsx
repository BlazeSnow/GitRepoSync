import { cleanup, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

// api 打桩：组件测试不触碰后端
vi.mock("@/lib/api", () => ({
  api: {
    getMcpConfig: vi
      .fn()
      .mockResolvedValue({ apiKey: "grs_test_key", exePath: "/opt/git-repo-sync" }),
  },
}));

// i18n 词典初始化通常由 main.tsx 引入；组件测试需显式导入副作用模块
import "@/i18n";
import i18next from "i18next";
import { McpPage } from "./McpPage";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

beforeEach(async () => {
  vi.clearAllMocks();
  await i18next.changeLanguage("zh");
});

it("渲染 API Key、客户端配置示例与全部 11 个工具", async () => {
  render(<McpPage token="tok" />);

  expect(await screen.findByText("grs_test_key")).toBeInTheDocument();
  // 配置示例包含可执行文件路径与 stdio 参数
  expect(screen.getByText(/\/opt\/git-repo-sync/)).toBeInTheDocument();

  for (const tool of [
    "list_repos",
    "discover_repos",
    "add_repo",
    "update_repo",
    "remove_repo",
    "sync_repo",
    "sync_repos",
    "get_sync_status",
    "list_logs",
    "get_base_dir",
    "set_base_dir",
  ]) {
    expect(screen.getByText(tool), `工具 ${tool} 应在表中`).toBeInTheDocument();
  }
});
