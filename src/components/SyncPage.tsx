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
import { cn } from "@/lib/utils";

const STALE_DAYS = [1, 3, 7, 30];

/** 可排序表头样式：保留吸顶、列宽与背景，加指针提示；激活排序时前景高亮 */
function sortHeaderClass(width: string, active: boolean): string {
  return cn(
    "sticky top-0 z-10 bg-card cursor-pointer select-none",
    width,
    active && "text-foreground",
  );
}

/** 同步范围的 localStorage 键：切页（组件卸载）后保持上次选择 */
const STALE_RANGE_KEY = "grs_stale_range";

/** 读取持久化的同步范围，非法值回落 all */
function loadStaleRange(): string {
  const v = localStorage.getItem(STALE_RANGE_KEY);
  if (v === null) return "all";
  return v === "all" || STALE_DAYS.map(String).includes(v) ? v : "all";
}

/** 只有 origin（没有任何备份目标）或连源地址都没有的仓库视为未配置，不参与同步 */
const isUnconfigured = (r: Repo) => !r.source || r.targets.length === 0;

/**
 * 同步范围过滤（与后端 select_stale_ids 语义一致）：all 显示全部；
 * N 天范围内仅保留已配置且「从未同步或上次同步早于 N 天前」的仓库。
 * 表格与「开始同步」按钮的计数共用同一份过滤结果，保证所见即可同步
 */
export function filterStale(repos: Repo[], stale: string): Repo[] {
  if (stale === "all") return repos;
  const days = Number(stale);
  if (!Number.isFinite(days) || days <= 0) return repos;
  const cutoff = Date.now() - days * 86400_000;
  return repos.filter(
    (r) => !isUnconfigured(r) && (r.lastSynced === null || r.lastSynced < cutoff),
  );
}

/** 可排序的列；表格排序状态 key 为 null 表示默认（后端返回的名称升序） */
export type SortKey = "name" | "status" | "lastSynced";
export interface SortSpec {
  key: SortKey | null;
  dir: "asc" | "desc";
}

/** 状态排序权重（升序 = 问题优先）：失败 > 同步中 > 已停止 > 未同步 > 成功 */
const STATUS_RANK: Record<string, number> = {
  failed: 0,
  running: 1,
  stopped: 2,
  idle: 3,
  success: 4,
};

/**
 * 表格排序（纯前端，作用于范围过滤后的可见行）：
 * - 名称：与后端默认一致的字符串序（升序与默认相同），降序反转；
 * - 状态：升序按问题优先（失败 > 同步中 > 已停止 > 未同步 > 成功），降序反转；
 * - 上次同步：升序「从未同步」最先、其后按时间从旧到新，降序相反；
 * - 未配置仓库不参与方向反转，固定排在最后；
 * - 同分时保持后端的名称顺序（Array.prototype.sort 稳定）
 */
export function sortRepos(repos: Repo[], sort: SortSpec): Repo[] {
  if (sort.key === null) return repos;
  const sign = sort.dir === "asc" ? 1 : -1;
  const configured: Repo[] = [];
  const unconfiguredRows: Repo[] = [];
  for (const r of repos) (isUnconfigured(r) ? unconfiguredRows : configured).push(r);
  configured.sort((a, b) => {
    let cmp: number;
    if (sort.key === "name") {
      // 与 SQLite ORDER BY name（UTF-8 字节序）保持一致的字符串比较
      cmp = a.name === b.name ? 0 : a.name < b.name ? -1 : 1;
    } else if (sort.key === "lastSynced") {
      const av = a.lastSynced ?? Number.NEGATIVE_INFINITY;
      const bv = b.lastSynced ?? Number.NEGATIVE_INFINITY;
      cmp = av === bv ? 0 : av < bv ? -1 : 1;
    } else {
      const av = STATUS_RANK[a.lastStatus] ?? 99;
      const bv = STATUS_RANK[b.lastStatus] ?? 99;
      cmp = av === bv ? 0 : av < bv ? -1 : 1;
    }
    return cmp * sign;
  });
  return [...configured, ...unconfiguredRows];
}

/** 表头点击的三态切换：未排 → 升序 → 降序 → 恢复默认名称序 */
export function toggleSort(current: SortSpec, key: SortKey): SortSpec {
  if (current.key !== key) return { key, dir: "asc" };
  if (current.dir === "asc") return { key, dir: "desc" };
  return { key: null, dir: "asc" };
}

export function SyncPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [repos, setRepos] = useState<Repo[]>([]);
  const [stale, setStaleState] = useState(loadStaleRange);
  // 范围选择写入 localStorage：切到日志等页面再回来时保持，不重置为全部
  const setStale = (v: string) => {
    setStaleState(v);
    localStorage.setItem(STALE_RANGE_KEY, v);
  };
  // 表格排序：默认按名称（后端返回顺序），点击状态/时间表头切换
  const [sort, setSort] = useState<SortSpec>({ key: null, dir: "asc" });
  // 编辑弹窗：null 表示添加，Repo 表示编辑；null 外层表示关闭
  const [editor, setEditor] = useState<{ repo: Repo | null } | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Repo | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; repo: Repo } | null>(null);
  const [error, setError] = useState("");
  const [refreshing, setRefreshing] = useState(false);

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

  // 范围过滤结果再按表头选择排序：表格展示与「开始同步」的 id 列表共用
  const visibleRepos = useMemo(
    () => sortRepos(filterStale(repos, stale), sort),
    [repos, stale, sort],
  );

  const staleIds = useMemo(
    () =>
      visibleRepos
        .filter((r) => stale !== "all" || !isUnconfigured(r))
        .map((r) => r.id),
    [visibleRepos, stale],
  );

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

  // 手动刷新：重新扫描基地址并加载最新列表（MCP 等其他入口的改动借此可见）
  async function handleRefresh() {
    if (refreshing) return;
    setRefreshing(true);
    try {
      await load();
    } finally {
      setRefreshing(false);
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
    // 阻止冒泡到 window 的菜单关闭监听：连续右键另一行时菜单直接切换而非消失
    e.stopPropagation();
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
            void api
              .startSync(token, [menu.repo.id])
              .then(load)
              .catch((err) => setError(String(err)));
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
        {/* 开始/停止按同步状态互斥切换：同一时刻只显示其中一个 */}
        {running ? (
          <Button variant="outline" onClick={handleStopSync}>
            <IconSquare className="h-3.5 w-3.5" />
            {t("stopSync")}
          </Button>
        ) : (
          <Button onClick={handleStartSync} disabled={staleIds.length === 0}>
            <IconRefresh />
            {stale === "all" ? t("startSync") : t("startSyncCount", { count: staleIds.length })}
          </Button>
        )}
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
        <Button variant="outline" onClick={() => void handleRefresh()} disabled={refreshing}>
          <IconRefresh />
          {t("refreshRepos")}
        </Button>
        <Button variant="secondary" onClick={() => setEditor({ repo: null })}>
          <IconPlus />
          {t("addRepo")}
        </Button>
      </div>

      {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

      {/* 容器级阻止右键默认行为：表头/空白区右键不再弹出 WebView 原生菜单 */}
      <div
        className="min-h-0 flex-1 overflow-hidden rounded-lg border bg-card"
        onContextMenu={(e) => e.preventDefault()}
      >
        <Table className="border-separate border-spacing-0">
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead
                className={sortHeaderClass("w-44", sort.key === "name")}
                aria-sort={
                  sort.key === "name" ? (sort.dir === "asc" ? "ascending" : "descending") : "none"
                }
                title={t("sortHint")}
                onClick={() => setSort((s) => toggleSort(s, "name"))}
              >
                {t("colRepo")}
                {sort.key === "name" && <span className="ml-1">{sort.dir === "asc" ? "↑" : "↓"}</span>}
              </TableHead>
              <TableHead className="sticky top-0 z-10 bg-card">{t("colAddress")}</TableHead>
              <TableHead
                className={sortHeaderClass("w-24", sort.key === "status")}
                aria-sort={
                  sort.key === "status" ? (sort.dir === "asc" ? "ascending" : "descending") : "none"
                }
                title={t("sortHint")}
                onClick={() => setSort((s) => toggleSort(s, "status"))}
              >
                {t("colStatus")}
                {sort.key === "status" && <span className="ml-1">{sort.dir === "asc" ? "↑" : "↓"}</span>}
              </TableHead>
              <TableHead
                className={sortHeaderClass("w-32", sort.key === "lastSynced")}
                aria-sort={
                  sort.key === "lastSynced"
                    ? sort.dir === "asc"
                      ? "ascending"
                      : "descending"
                    : "none"
                }
                title={t("sortHint")}
                onClick={() => setSort((s) => toggleSort(s, "lastSynced"))}
              >
                {t("colLastSynced")}
                {sort.key === "lastSynced" && (
                  <span className="ml-1">{sort.dir === "asc" ? "↑" : "↓"}</span>
                )}
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {visibleRepos.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="h-32 text-center text-muted-foreground">
                  {stale === "all" ? t("syncEmpty") : t("staleEmpty")}
                </TableCell>
              </TableRow>
            ) : (
              visibleRepos.map((repo) => {
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
          onDelete={(r) => {
            // 弹窗内的删除入口：关闭编辑，转由既有确认弹窗执行删除
            setEditor(null);
            setDeleteTarget(r);
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
