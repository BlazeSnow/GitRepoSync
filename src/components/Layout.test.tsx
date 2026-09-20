import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { Layout } from "./Layout";

afterEach(cleanup);

beforeEach(async () => {
  vi.clearAllMocks();
  // 先 import @/i18n 触发实例初始化（资源注册），再切语言
  await import("@/i18n");
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
});

it("渲染侧边栏：版本号、当前用户、导航项与当前页高亮", () => {
  render(
    <Layout page="sync" onNavigate={vi.fn()} username="admin" version="1.0.0" onLogout={vi.fn()}>
      <div>content</div>
    </Layout>,
  );

  expect(screen.getByText("v1.0.0")).toBeInTheDocument();
  expect(screen.getByText("当前用户：admin")).toBeInTheDocument();
  for (const label of ["同步仓库", "日志", "MCP", "设置"]) {
    expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
  }
  // 当前页按钮高亮（bg-primary）
  expect(screen.getByRole("button", { name: "同步仓库" }).className).toContain("bg-primary");
  expect(screen.getByRole("button", { name: "日志" }).className).not.toContain("bg-primary");
});

it("点击导航项回调对应页面，退出登录触发回调", () => {
  const onNavigate = vi.fn();
  const onLogout = vi.fn();

  render(
    <Layout page="sync" onNavigate={onNavigate} username="admin" version="1.0.0" onLogout={onLogout}>
      <div>content</div>
    </Layout>,
  );

  fireEvent.click(screen.getByRole("button", { name: "设置" }));
  expect(onNavigate).toHaveBeenCalledWith("settings");
  fireEvent.click(screen.getByRole("button", { name: "退出登录" }));
  expect(onLogout).toHaveBeenCalled();
});
