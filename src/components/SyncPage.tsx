import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { Repo, SyncEvent, SyncStatus } from "@/lib/types";
import { useTranslation } from "react-i18next";
import { relativeTime } from "@/i18n";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
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
import { RepoEditDialog } from "@/components/RepoEditDialog";
import { DeleteRepoDialog } from "@/components/DeleteRepoDialog";
import { IconPlus, IconRefresh, IconSquare } from "@/components/icons";

const STALE_DAYS = [1, 3, 7, 30];

/** 只有 origin（没有任何备份目标）或连源地址都没有的仓库视为未配置，不参与同步 */
const isUnconfigured = (r: Repo) => !r.source || r.targets.length === 0;

export function SyncPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [repos, setRepos] = useState<Repo[]>([]);
  const [stale, setStale] = useState("all");
  // 编辑弹窗：null 表示添加，Repo 表示编辑；null 外层表示关闭
  const [editor, setEditor] = useState<{ repo: Repo | null } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Repo | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; repo: Repo } | null>(null);
  const [error, setError] = useState("");

  // 全量发现（扫描基地址 + 登记新仓库）：仅在页面挂载和手动刷新时执行
  const load = useCallback(async () => {
    try {
      setRepos(await api.discoverRepos(token));
      setError("");
    } catch (err) {
      setError(String(err));
    }
  }, [token]);

  // 轻量刷新（纯 SQL 查询）：同步事件高频触发时按 400ms 节流，避免大表格反复重渲
  const reloadList = useCallback(async () => {
    try {
      setRepos(await api.listRepos(token));
      setError("");
    } catch (err) {
      setError(String(err));
    }
  }, [token]);
  const reloadTimer = useRef<number | null>(null);
  const scheduleReload = useCallback(() => {
    if (reloadTimer.current !== null) return;
    reloadTimer.current = window.setTimeout(() => {
      reloadTimer.current = null;
      void reloadList();
    }, 400);
  }, [reloadList]);

  useEffect(() => {
    void load();
    const unlisten = listen<SyncEvent>("sync-status", () => {
      scheduleReload();
    });
    return () => {
      void unlisten.then((f) => f());
      if (reloadTimer.current !== null) window.clearTimeout(reloadTimer.current);
    };
  }, [load]);

  const staleIds = useMemo(() => {
    const days = stale === "all" ? 0 : Number(stale);
    const inRange = (r: Repo) =>
      days === 0 || r.lastSynced === null || r.lastSynced < Date.now() - days * 86400_000;
    return repos.filter((r) => !isUnconfigured(r) && inRange(r)).map((r) => r.id);
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
          onSelect: () => setEditor({ repo: menu.repo }),
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

  // 地址列：origin 与全部备份目标压缩在一个单元格内，每行「远端名: 地址」
  const remoteCell = (repo: Repo) => {
    const lines: { remote: string; url: string; tip?: string }[] = [
      { remote: "origin", url: repo.source },
      ...repo.targets.map((tg) => ({
        remote: tg.remote,
        url: tg.url,
        tip: [tg.lastMessage, tg.lastStatus].filter(Boolean).join(" | "),
      })),
    ];
    return (
      <div className="space-y-0.5">
        {lines.map((l) => (
          <div
            key={l.remote}
            className="truncate text-muted-foreground"
            title={l.tip || undefined}
          >
            {l.remote}: {l.url || t("notConfigured")}
          </div>
        ))}
      </div>
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
        <Button variant="secondary" onClick={() => setEditor({ repo: null })}>
          <IconPlus />
          {t("addRepo")}
        </Button>
      </div>

      {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

      <div className="min-h-0 flex-1 overflow-hidden rounded-lg border bg-card">
        <Table className="border-separate border-spacing-0">
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="sticky top-0 z-10 w-44 bg-card">{t("colRepo")}</TableHead>
              <TableHead className="sticky top-0 z-10 bg-card">{t("colAddress")}</TableHead>
              <TableHead className="sticky top-0 z-10 w-24 bg-card">{t("colStatus")}</TableHead>
              <TableHead className="sticky top-0 z-10 w-32 bg-card">{t("colLastSynced")}</TableHead>
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
                    onDoubleClick={() => setEditor({ repo })}
                    onContextMenu={(e) => openMenu(e, repo)}
                    title={repo.lastMessage ?? undefined}
                  >
                    <TableCell className="font-medium">{repo.name}</TableCell>
                    <TableCell>{remoteCell(repo)}</TableCell>
                    <TableCell>
                      {isUnconfigured(repo) ? (
                        <Badge variant="secondary">{t("notConfigured")}</Badge>
                      ) : (
                        <Badge
                          variant={badge.variant}
                          className={repo.lastStatus === "running" ? "animate-pulse" : ""}
                        >
                          {badge.label}
                        </Badge>
                      )}
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

      {editor && (
        <RepoEditDialog
          token={token}
          repo={editor.repo}
          onClose={() => setEditor(null)}
          onSaved={() => {
            setEditor(null);
            void load();
          }}
        />
      )}

      {deleteTarget && (
        <DeleteRepoDialog
          repo={deleteTarget}
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => void handleDelete()}
        />
      )}
    </div>
  );
}
