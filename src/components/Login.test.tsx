import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

// api 模块打桩：组件测试不触碰 Tauri invoke
vi.mock("@/lib/api", () => ({
  api: { login: vi.fn() },
}));

// i18n 词典初始化通常由 main.tsx 引入；组件测试需显式导入副作用模块
import "@/i18n";
import i18next from "i18next";
import { api } from "@/lib/api";
import { Login } from "./Login";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

beforeEach(async () => {
  vi.mocked(api.login).mockReset();
  await i18next.changeLanguage("zh");
});

it("渲染登录表单，默认用户名 admin 且保持登录勾选", () => {
  render(<Login onLogin={() => {}} />);
  expect(screen.getByLabelText("用户名")).toHaveValue("admin");
  expect(screen.getByLabelText("密码")).toHaveValue("");
  expect(screen.getByRole("checkbox", { name: "保持登录 30 天" })).toBeChecked();
  expect(screen.getByRole("button", { name: "登录" })).toBeEnabled();
});

it("提交表单调用 api.login 并透传结果给 onLogin", async () => {
  const onLogin = vi.fn();
  vi.mocked(api.login).mockResolvedValueOnce({ token: "tok-1", username: "admin" });
  render(<Login onLogin={onLogin} />);

  fireEvent.change(screen.getByLabelText("密码"), { target: { value: "admin123" } });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await vi.waitFor(() => expect(onLogin).toHaveBeenCalled());

  expect(api.login).toHaveBeenCalledWith("admin", "admin123", true);
  expect(onLogin).toHaveBeenCalledWith("tok-1", "admin");
  expect(screen.queryByText("用户名或密码错误")).toBeNull();
});

it("登录失败显示错误信息且不调用 onLogin，可重试", async () => {
  const onLogin = vi.fn();
  vi.mocked(api.login).mockRejectedValueOnce("用户名或密码错误");
  render(<Login onLogin={onLogin} />);

  fireEvent.change(screen.getByLabelText("密码"), { target: { value: "wrong" } });
  fireEvent.click(screen.getByRole("button", { name: "登录" }));

  expect(await screen.findByText("用户名或密码错误")).toBeInTheDocument();
  expect(onLogin).not.toHaveBeenCalled();
  // 失败后按钮恢复可用
  expect(screen.getByRole("button", { name: "登录" })).toBeEnabled();
});
