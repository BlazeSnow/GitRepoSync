import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it } from "vitest";
import { ToastProvider, useToast } from "./Toast";

// i18n 词典初始化通常由 main.tsx 引入；组件测试需显式导入副作用模块
import "@/i18n";

// vitest 非 globals 模式下 testing-library 不自动卸载，需手动清理
afterEach(cleanup);

beforeEach(async () => {
  localStorage.clear();
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
});

function Harness({
  kind,
  title,
  description,
}: {
  kind: "success" | "error" | "info";
  title: string;
  description?: string;
}) {
  const toast = useToast();
  return <button onClick={() => toast({ kind, title, description })}>push</button>;
}

it("弹出 toast：标题、描述与关闭按钮呈现，点击关闭移除", async () => {
  render(
    <ToastProvider>
      <Harness kind="error" title="同步失败" description="推送到 backup 失败" />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "push" }));
  expect(screen.getByText("同步失败")).toBeInTheDocument();
  expect(screen.getByText("推送到 backup 失败")).toBeInTheDocument();
  // 进出场为 motion 动画：移除在 waitFor 内完成
  fireEvent.click(screen.getByRole("button", { name: "关闭" }));
  await waitFor(() => expect(screen.queryByText("同步失败")).not.toBeInTheDocument());
});

it("同屏最多 5 条，超出挤掉最旧的（批量同步不刷屏）", () => {
  function MultiHarness() {
    const toast = useToast();
    return (
      <button
        onClick={() => {
          for (let i = 1; i <= 6; i++) toast({ kind: "info", title: `T${i}` });
        }}
      >
        push6
      </button>
    );
  }
  render(
    <ToastProvider>
      <MultiHarness />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "push6" }));
  expect(screen.queryByText("T1")).not.toBeInTheDocument();
  expect(screen.getByText("T6")).toBeInTheDocument();
  expect(screen.getAllByText(/^T\d$/)).toHaveLength(5);
});

it("自动消失：时长经 dismissMs / errorDismissMs 注入（测试用短时长）", async () => {
  render(
    <ToastProvider dismissMs={50} errorDismissMs={80}>
      <Harness kind="info" title="自动消失" />
    </ToastProvider>,
  );
  fireEvent.click(screen.getByRole("button", { name: "push" }));
  expect(screen.getByText("自动消失")).toBeInTheDocument();
  await waitFor(() => expect(screen.queryByText("自动消失")).not.toBeInTheDocument());
});
