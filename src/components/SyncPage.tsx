import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { Repo, SyncEvent, SyncStatus } from "@/lib/types";
import { formatRelative } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { ContextMenu, type ContextMenuItem } from "@/components/ContextMenu";
import { IconPlus, IconRefresh, IconSquare } from "@/components/icons";

const STALE_OPTIONS = [
  { value: "all", label: "全部仓库", days: 0 },
  { value: "1", label: "1 天内未同步", days: 1 },
  { value: "3", label: "3 天内未同步", days: 3 },
  { value: "7", label: "7 天内未同步", days: 7 },
  { value: "30", label: "30 天内未同步", days: 30 },
];

const STATUS_BADGE: Record<SyncStatus, { label: string; variant: "secondary" | "success" | "destructive" | "outline" | "default" }> = {
  idle: { label: "未同步", variant: "outline" },
  running: { label: "同步中", variant: "secondary" },
  success: { label: "成功", variant: "success" },
  failed: { label: "失败", variant: "destructive" },
  stopped: { label: "已停止", variant: "outline" },
};

interface EditState {
  id: string | null;
  name: string;
  source: string;
  target: string;
}

export function SyncPage({ token }: { token: string }) {
  const [repos, setRepos] = useState<Repo[]>([]);
  const [stale, setStale] = useState("all");
  const [edit, setEdit] = useState<EditState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Repo | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; repo: Repo } | null>(null);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setRepos(await api.listRepos(token));
    } catch (err) {
      setError(String(err));
    }
  }, [token]);

  useEffect(() => {
    void load();
    const unlisten = listen<SyncEvent>("sync-status", () => {
      void load();
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [load]);

  const staleIds = useMemo(() => {
    const opt = STALE_OPTIONS.find((o) => o.value === stale);
    if (!opt || opt.days === 0) return repos.map((r) => r.id);
    const threshold = Date.now() - opt.days * 86400_000;
    return repos
      .filter((r) => r.lastSynced === null || r.lastSynced < threshold)
      .map((r) => r.id);
  }, [repos, stale]);

  async function handleStartSync() {
    setError("");
    if (staleIds.length === 0) return;
    try {
      await api.startSync(token, staleIds);
      void load();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleStopSync() {
    setError("");
    try {
      await api.stopSync(token, null);
      void load();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleSave() {
    if (!edit) return;
    setError("");
    try {
      await api.saveRepo(token, {
        id: edit.id,
        name: edit.name,
        source: edit.source,
        target: edit.target,
      });
      setEdit(null);
      void load();
    } catch (err) {
      setError(String(err));
    }
  }

  async function handleDelete() {
    if (!deleteTarget) return;
    setError("");
    try {
      await api.deleteRepo(token, deleteTarget.id);
      setDeleteTarget(null);
      void load();
    } catch (err) {
      setDeleteTarget(null);
      setError(String(err));
    }
  }

  function openMenu(e: React.MouseEvent, repo: Repo) {
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, repo });
  }

  const menuItems: ContextMenuItem[] = menu
    ? [
        { label: "编辑", onSelect: () => setEdit({ id: menu.repo.id, name: menu.repo.name, source: menu.repo.source, target: menu.repo.target }) },
        {
          label: "立即同步",
          onSelect: () => {
            void api.startSync(token, [menu.repo.id]).then(load);
          },
        },
        { label: "删除", danger: true, onSelect: () => setDeleteTarget(menu.repo) },
      ]
    : [];

  const running = repos.some((r) => r.lastStatus === "running");

  return (
    <div className="flex h-full flex-col p-6">
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <Button onClick={handleStartSync} disabled={staleIds.length === 0}>
          <IconRefresh />
          开始同步{stale !== "all" ? `（${staleIds.length} 个）` : ""}
        </Button>
        <Button variant="outline" onClick={handleStopSync} disabled={!running}>
          <IconSquare className="h-3.5 w-3.5" />
          停止同步
        </Button>
        <Select value={stale} onValueChange={setStale}>
          <SelectTrigger className="w-44">
            <SelectValue placeholder="选择同步范围" />
          </SelectTrigger>
          <SelectContent>
            {STALE_OPTIONS.map((o) => (
              <SelectItem key={o.value} value={o.value}>
                {o.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex-1" />
        <Button
          variant="secondary"
          onClick={() => setEdit({ id: null, name: "", source: "", target: "" })}
        >
          <IconPlus />
          添加仓库
        </Button>
      </div>

      {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

      <div className="min-h-0 flex-1 rounded-lg border bg-card">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-52">仓库</TableHead>
              <TableHead>源地址</TableHead>
              <TableHead>目标地址</TableHead>
              <TableHead className="w-24">状态</TableHead>
              <TableHead className="w-32">上次同步</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {repos.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="h-32 text-center text-muted-foreground">
                  暂无仓库，点击右上角“添加仓库”开始
                </TableCell>
              </TableRow>
            ) : (
              repos.map((repo) => {
                const badge = STATUS_BADGE[repo.lastStatus] ?? STATUS_BADGE.idle;
                return (
                  <TableRow
                    key={repo.id}
                    className="cursor-default select-none"
                    onDoubleClick={() =>
                      setEdit({ id: repo.id, name: repo.name, source: repo.source, target: repo.target })
                    }
                    onContextMenu={(e) => openMenu(e, repo)}
                    title={repo.lastMessage ?? undefined}
                  >
                    <TableCell className="font-medium">{repo.name}</TableCell>
                    <TableCell className="max-w-0 truncate text-muted-foreground" title={repo.source}>
                      {repo.source}
                    </TableCell>
                    <TableCell className="max-w-0 truncate text-muted-foreground" title={repo.target}>
                      {repo.target}
                    </TableCell>
                    <TableCell>
                      <Badge variant={badge.variant} className={repo.lastStatus === "running" ? "animate-pulse" : ""}>
                        {badge.label}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">{formatRelative(repo.lastSynced)}</TableCell>
                  </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      </div>

      {menu && <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => setMenu(null)} />}

      <Dialog open={edit !== null} onOpenChange={(open) => !open && setEdit(null)}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{edit?.id ? "编辑仓库" : "添加仓库"}</DialogTitle>
            <DialogDescription>
              目标地址将以 git 镜像（--mirror）方式保存，请填写一个本地路径
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="repo-name">仓库名称</Label>
              <Input
                id="repo-name"
                value={edit?.name ?? ""}
                onChange={(e) => setEdit((s) => (s ? { ...s, name: e.target.value } : s))}
                placeholder="my-repo"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="repo-source">源地址</Label>
              <Input
                id="repo-source"
                value={edit?.source ?? ""}
                onChange={(e) => setEdit((s) => (s ? { ...s, source: e.target.value } : s))}
                placeholder="https://github.com/user/repo.git"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="repo-target">目标地址</Label>
              <Input
                id="repo-target"
                value={edit?.target ?? ""}
                onChange={(e) => setEdit((s) => (s ? { ...s, target: e.target.value } : s))}
                placeholder="D:\backup\my-repo.git"
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEdit(null)}>
              取消
            </Button>
            <Button onClick={handleSave}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>删除仓库</DialogTitle>
            <DialogDescription>
              确定要删除仓库“{deleteTarget?.name}”吗？仅移除记录，不会删除本地文件。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              取消
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              删除
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
