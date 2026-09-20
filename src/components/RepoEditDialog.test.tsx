import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Repo } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: { saveRepo: vi.fn() },
}));

import { api } from "@/lib/api";
import { RepoEditDialog } from "./RepoEditDialog";

afterEach(cleanup);

beforeEach(async () => {
  vi.clearAllMocks();
  // 先 import @/i18n 触发实例初始化（资源注册），再切语言
  await import("@/i18n");
  const { default: i18next } = await import("i18next");
  await i18next.changeLanguage("zh");
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

it("添加模式：目标行默认远端名 backup，保存时过滤空目标并提交", async () => {
  vi.mocked(api.saveRepo).mockResolvedValue(repo({}));

  render(<RepoEditDialog token="tok" repo={null} onClose={vi.fn()} onSaved={vi.fn()} />);

  fireEvent.change(screen.getByLabelText("仓库名称"), { target: { value: "new-repo" } });
  fireEvent.change(screen.getByLabelText("源地址"), { target: { value: "https://src/new.git" } });
  fireEvent.change(screen.getByPlaceholderText("远端名"), { target: { value: "backup" } });
  fireEvent.change(screen.getByPlaceholderText("目标地址"), { target: { value: "https://bak/new.git" } });
  fireEvent.click(screen.getByRole("button", { name: "保存" }));

  await waitFor(() => expect(api.saveRepo).toHaveBeenCalled());
  expect(api.saveRepo).toHaveBeenCalledWith("tok", {
    id: null,
    name: "new-repo",
    source: "https://src/new.git",
    targets: [{ remote: "backup", url: "https://bak/new.git" }],
  });
});

it("编辑模式：预填仓库与目标，保存回调触发", async () => {
  vi.mocked(api.saveRepo).mockResolvedValue(repo({}));
  const onSaved = vi.fn();

  render(
    <RepoEditDialog
      token="tok"
      repo={repo({
        targets: [
          {
            remote: "backup",
            url: "https://bak/demo.git",
            lastStatus: "idle",
            lastMessage: null,
            lastSynced: null,
          },
        ],
      })}
      onClose={vi.fn()}
      onSaved={onSaved}
    />,
  );

  expect(screen.getByLabelText("仓库名称")).toHaveValue("demo");
  expect(screen.getByLabelText("源地址")).toHaveValue("https://github.com/u/demo.git");
  expect(screen.getByPlaceholderText("目标地址")).toHaveValue("https://bak/demo.git");

  fireEvent.click(screen.getByRole("button", { name: "保存" }));
  await waitFor(() => expect(onSaved).toHaveBeenCalled());
  expect(api.saveRepo).toHaveBeenCalledWith(
    "tok",
    expect.objectContaining({ id: "id-1" }),
  );
});

it("目标行可增删：删除后仅剩一行，「添加目标」追加空行", () => {
  render(
    <RepoEditDialog
      token="tok"
      repo={repo({
        targets: [
          { remote: "a", url: "https://a.git", lastStatus: "idle", lastMessage: null, lastSynced: null },
          { remote: "b", url: "https://b.git", lastStatus: "idle", lastMessage: null, lastSynced: null },
        ],
      })}
      onClose={vi.fn()}
      onSaved={vi.fn()}
    />,
  );

  expect(screen.getAllByPlaceholderText("远端名")).toHaveLength(2);

  // 删除第一行（标题为「删除」的图标按钮）
  fireEvent.click(screen.getAllByTitle("删除")[0]);
  expect(screen.getAllByPlaceholderText("远端名")).toHaveLength(1);

  fireEvent.click(screen.getByRole("button", { name: "添加目标" }));
  expect(screen.getAllByPlaceholderText("远端名")).toHaveLength(2);
});

it("保存失败显示后端错误，取消按钮关闭弹窗", async () => {
  vi.mocked(api.saveRepo).mockRejectedValue("仓库名称已存在");
  const onClose = vi.fn();
  const onSaved = vi.fn();

  render(<RepoEditDialog token="tok" repo={null} onClose={onClose} onSaved={onSaved} />);

  fireEvent.change(screen.getByLabelText("仓库名称"), { target: { value: "dup" } });
  fireEvent.change(screen.getByLabelText("源地址"), { target: { value: "https://src.git" } });
  fireEvent.click(screen.getByRole("button", { name: "保存" }));

  expect(await screen.findByText("仓库名称已存在")).toBeInTheDocument();
  expect(onSaved).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole("button", { name: "取消" }));
  expect(onClose).toHaveBeenCalled();
});
