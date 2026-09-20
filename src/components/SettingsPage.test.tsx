import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

// api 与系统对话框打桩：组件测试不触碰后端与原生对话框
vi.mock("@/lib/api", () => ({
  api: {
    getAppInfo: vi
      .fn()
      .mockResolvedValue({ name: "GitRepoSync", version: "0.0.0-test", os: "test" }),
    getBaseDir: vi.fn().mockResolvedValue("/home/u/repo"),
    setBaseDir: vi.fn(),
    changePassword: vi.fn(),
    regenerateMcpKey: vi.fn(),
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

// i18n 词典初始化通常由 main.tsx 引入；组件测试需显式导入副作用模块
import "@/i18n";
import i18next from "i18next";
import { SettingsPage } from "./SettingsPage";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

beforeEach(async () => {
  vi.clearAllMocks();
  localStorage.clear();
  document.documentElement.className = "";
  // 主题检测依赖 matchMedia：桩为系统浅色
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    })),
  );
  await i18next.changeLanguage("zh");
});

afterEach(() => {
  vi.unstubAllGlobals();
});

it("渲染设置各分区：外观、语言、基地址、账户、软件信息", async () => {
  render(<SettingsPage token="tok" username="admin" />);
  expect(await screen.findByText("外观")).toBeInTheDocument();
  expect(screen.getByText("语言 / Language")).toBeInTheDocument();
  expect(screen.getByText("仓库基地址")).toBeInTheDocument();
  expect(screen.getByText("账户")).toBeInTheDocument();
  expect(screen.getByText("软件信息")).toBeInTheDocument();
});

it("修改密码表单具备密码管理器语义：form 提交与 autocomplete 标注", async () => {
  render(<SettingsPage token="tok" username="admin" />);

  // readonly 用户名字段关联凭据，新旧密码用 current/new-password 区分
  const userInput = screen.getByLabelText("用户名") as HTMLInputElement;
  expect(userInput).toHaveAttribute("autocomplete", "username");
  expect(userInput).toHaveValue("admin");
  expect(userInput).toHaveAttribute("readonly");
  expect(screen.getByLabelText("旧密码")).toHaveAttribute("autocomplete", "current-password");
  expect(screen.getByLabelText("新密码")).toHaveAttribute("autocomplete", "new-password");
  expect(screen.getByLabelText("确认新密码")).toHaveAttribute("autocomplete", "new-password");

  // form 提交：回车或点击提交按钮触发 changePassword
  fireEvent.change(screen.getByLabelText("旧密码"), { target: { value: "admin123" } });
  fireEvent.change(screen.getByLabelText("新密码"), { target: { value: "newpass1" } });
  fireEvent.change(screen.getByLabelText("确认新密码"), { target: { value: "newpass1" } });
  const form = userInput.closest("form");
  expect(form).not.toBeNull();
  fireEvent.submit(form as HTMLFormElement);
  const { api } = await import("@/lib/api");
  await waitFor(() =>
    expect(api.changePassword).toHaveBeenCalledWith("tok", "admin123", "newpass1"),
  );
});

it("主题卡片：点击深色立即应用 .dark 并持久化，点击浅色恢复", async () => {
  render(<SettingsPage token="tok" username="admin" />);
  const darkButton = await screen.findByRole("button", { name: "深色" });
  fireEvent.click(darkButton);
  expect(document.documentElement.classList.contains("dark")).toBe(true);
  expect(localStorage.getItem("grs_theme")).toBe("dark");

  fireEvent.click(screen.getByRole("button", { name: "浅色" }));
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  expect(localStorage.getItem("grs_theme")).toBe("light");

  // 跟随系统：系统为浅色（桩）时不加深色类
  fireEvent.click(screen.getByRole("button", { name: "跟随系统" }));
  expect(document.documentElement.classList.contains("dark")).toBe(false);
  expect(localStorage.getItem("grs_theme")).toBe("system");
});
