import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Repo } from "@/lib/types";
import { RepoDetailDialog } from "./RepoDetailDialog";

// i18n 词典初始化通常由 main.tsx 引入；组件测试需显式导入副作用模块
import "@/i18n";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

beforeEach(async () => {
  localStorage.clear();
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
});

const repo: Repo = {
  id: "id-1",
  name: "demo",
  source: "https://github.com/u/demo.git",
  lastSynced: null,
  lastStatus: "idle",
  lastMessage: null,
  targets: [
    { remote: "backup", url: "https://gitlab.com/u/demo.git", lastStatus: "idle", lastMessage: null, lastSynced: null },
  ],
};

it("只读详情：标题为仓库名，源与目标只读展示，无任何输入与保存按钮", () => {
  render(<RepoDetailDialog repo={repo} onClose={vi.fn()} />);
  expect(screen.getByText("demo")).toBeInTheDocument();
  expect(screen.getByText("https://github.com/u/demo.git")).toBeInTheDocument();
  expect(screen.getByText("https://gitlab.com/u/demo.git")).toBeInTheDocument();
  // 只读：无输入框、无保存按钮，也无删除入口（删除已随仓库代管一起移除）
  expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "删除仓库" })).not.toBeInTheDocument();
});

it("关闭回调", () => {
  const onClose = vi.fn();
  render(<RepoDetailDialog repo={repo} onClose={onClose} />);
  fireEvent.click(screen.getByRole("button", { name: "关闭" }));
  expect(onClose).toHaveBeenCalledTimes(1);
});

it("无目标的仓库显示「未配置」占位", () => {
  render(<RepoDetailDialog repo={{ ...repo, targets: [] }} onClose={vi.fn()} />);
  expect(screen.getAllByText("未配置").length).toBeGreaterThan(0);
});
