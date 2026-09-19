import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { Repo, SyncEvent, SyncStatus } from "@/lib/types";
import { useTranslation } from "react-i18next";
import { relativeTime } from "@/i18n";
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
import { IconPlus, IconRefresh, IconSquare, IconX } from "@/components/icons";

const STALE_DAYS = [1, 3, 7, 30];

export function SyncPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [repos, setRepos] = useState<Repo[]>([]);
  const [stale, setStale] = useState("all");
  const [edit, setEdit] = useState<{
    id: string | null;
    name: string;
    source: string;
  } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Repo | null>(null);
  const [editTargets, setEditTargets] = useState<{ remote: string; url: string }[]>([]);
  const [menu, setMenu] = useState<{ x: number; y: number; repo: Repo } | null>(null);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      // 自动发现基地址内仓库（origin → 源地址，backup → 目标地址）后返回全量列表
      setRepos(await api.discoverRepos(token));
      setError("");
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

  const openEditor = (repo: Repo | null) => {
    setEdit(
      repo
        ? { id: repo.id, name: repo.name, source: repo.source }
        : { id: null, name: "", source: "" },
    );
    setEditTargets(
      repo
        ? repo.targets.map((t) => ({ remote: t.remote, url: t.url }))
        : [{ remote: "backup", url: "" }],
    );
  };

  const staleIds = useMemo(() => {
    const days = stale === "all" ? 0 : Number(stale);
    if (days === 0) return repos.map((r) => r.id);
    const threshold = Date.now() - days * 86400_000;
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
        targets: editTargets.filter((t) => t.remote.trim() || t.url.trim()),
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
        {
          label: t("editRepo"),
          onSelect: () => openEditor(menu.repo),
        },
        {
          label: t("startSync"),
          onSelect: () => {
            void api.startSync(token, [menu.repo.id]).then(load);
          },
        },
        { label: t("confirmDelete"), danger: true, onSelect: () => setDeleteTarget(menu.repo) },
      ]
    : [];

  const statusBadge: Record<
    SyncStatus,
    { label: string; variant: "secondary" | "success" | "destructive" | "outline" }
  > = {
    idle: { label: t("statusIdle"), variant: "outline" },
    running: { label: t("statusRunning"), variant: "secondary" },
    success: { label: t("statusSuccess"), variant: "success" },
    failed: { label: t("statusFailed"), variant: "destructive" },
    stopped: { label: t("statusStopped"), variant: "outline" },
  };

  const running = repos.some((r) => r.lastStatus === "running");

  // 表头目标列：全部仓库出现过的备份远端名（并集）
  const targetRemotes = useMemo(() => {
    const set = new Set<string>();
    repos.forEach((r) => r.targets.forEach((t) => set.add(t.remote)));
    return [...set].sort();
  }, [repos]);

  const targetCell = (repo: Repo, remote: string) => {
    const t = repo.targets.find((x) => x.remote === remote);
    if (!t) return <span className="text-muted-foreground/40">—</span>;
    const badge = statusBadge[t.lastStatus] ?? statusBadge.idle;
    return (
      <span title={t.lastMessage ?? undefined} className="inline-flex">
        <Badge
          variant={badge.variant}
          className={t.lastStatus === "running" ? "animate-pulse" : ""}
        >
          {badge.label}
        </Badge>
      </span>
    );
  };

  return (
    <div className="flex h-full flex-col p-6">
      <div className="mb-4 flex flex-wrap items-center gap-2">
        <Button onClick={handleStartSync} disabled={staleIds.length === 0}>
          <IconRefresh />
          {stale === "all" ? t("startSync") : t("startSyncCount", { count: staleIds.length })}
        </Button>
        <Button variant="outline" onClick={handleStopSync} disabled={!running}>
          <IconSquare className="h-3.5 w-3.5" />
          {t("stopSync")}
        </Button>
        <Select value={stale} onValueChange={setStale}>
          <SelectTrigger className="w-48">
            <SelectValue placeholder={t("staleAll")} />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">{t("staleAll")}</SelectItem>
            {STALE_DAYS.map((d) => (
              <SelectItem key={d} value={String(d)}>
                {t("staleDays", { count: d })}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex-1" />
        <Button variant="secondary" onClick={() => openEditor(null)}>
          <IconPlus />
          {t("addRepo")}
        </Button>
      </div>

      {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

      <div className="min-h-0 flex-1 rounded-lg border bg-card">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-44">{t("colRepo")}</TableHead>
              <TableHead className="max-w-56">{t("colSource")}</TableHead>
              {targetRemotes.map((remote) => (
                <TableHead key={remote} className="min-w-24">
                  <span className="font-mono text-xs">{remote}</span>
                </TableHead>
              ))}
              <TableHead className="w-24">{t("colStatus")}</TableHead>
              <TableHead className="w-32">{t("colLastSynced")}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {repos.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="h-32 text-center text-muted-foreground">
                  {t("syncEmpty")}
                </TableCell>
              </TableRow>
            ) : (
              repos.map((repo) => {
                const badge = statusBadge[repo.lastStatus] ?? statusBadge.idle;
                return (
                  <TableRow
                    key={repo.id}
                    className="cursor-default select-none"
                    onDoubleClick={() => openEditor(repo)}
                    onContextMenu={(e) => openMenu(e, repo)}
                    title={repo.lastMessage ?? undefined}
                  >
                    <TableCell className="font-medium">{repo.name}</TableCell>
                    <TableCell
                      className="max-w-0 truncate text-muted-foreground"
                      title={repo.source || undefined}
                    >
                      {repo.source || t("notConfigured")}
                    </TableCell>
                    {targetRemotes.map((remote) => (
                      <TableCell key={remote}>{targetCell(repo, remote)}</TableCell>
                    ))}
                    <TableCell>
                      <Badge
                        variant={badge.variant}
                        className={repo.lastStatus === "running" ? "animate-pulse" : ""}
                      >
                        {badge.label}
                      </Badge>
                    </TableCell>
                    <TableCell className="text-muted-foreground">
                      {relativeTime(repo.lastSynced)}
                    </TableCell>
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
            <DialogTitle>{edit?.id ? t("editRepo") : t("addRepoTitle")}</DialogTitle>
            <DialogDescription>{t("repoFlowDesc")}</DialogDescription>
          </DialogHeader>
          <div className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="repo-name">{t("fieldName")}</Label>
              <Input
                id="repo-name"
                value={edit?.name ?? ""}
                onChange={(e) => setEdit((s) => (s ? { ...s, name: e.target.value } : s))}
                placeholder={t("placeholderName")}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="repo-source">{t("fieldSource")}</Label>
              <Input
                id="repo-source"
                value={edit?.source ?? ""}
                onChange={(e) => setEdit((s) => (s ? { ...s, source: e.target.value } : s))}
                placeholder={t("placeholderSource")}
              />
            </div>
            <div className="space-y-1.5">
              <Label>{t("fieldTarget")}</Label>
              <div className="space-y-1.5">
                {editTargets.map((tg, i) => (
                  <div key={i} className="flex items-center gap-2">
                    <Input
                      className="w-28 shrink-0"
                      placeholder={t("targetRemote")}
                      value={tg.remote}
                      onChange={(e) =>
                        setEditTargets((ts) =>
                          ts.map((x, j) => (j === i ? { ...x, remote: e.target.value } : x)),
                        )
                      }
                    />
                    <Input
                      className="min-w-0 flex-1 font-mono text-xs"
                      placeholder={t("targetUrl")}
                      value={tg.url}
                      onChange={(e) =>
                        setEditTargets((ts) =>
                          ts.map((x, j) => (j === i ? { ...x, url: e.target.value } : x)),
                        )
                      }
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      title={t("confirmDelete")}
                      onClick={() => setEditTargets((ts) => ts.filter((_, j) => j !== i))}
                    >
                      <IconX />
                    </Button>
                  </div>
                ))}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setEditTargets((ts) => [...ts, { remote: "", url: "" }])}
                >
                  <IconPlus />
                  {t("addTarget")}
                </Button>
              </div>
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setEdit(null)}>
              {t("cancel")}
            </Button>
            <Button onClick={handleSave}>{t("save")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={deleteTarget !== null} onOpenChange={(open) => !open && setDeleteTarget(null)}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>{t("deleteRepo")}</DialogTitle>
            <DialogDescription>
              {deleteTarget && t("deleteRepoDesc", { name: deleteTarget.name })}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              {t("cancel")}
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              {t("confirmDelete")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}


