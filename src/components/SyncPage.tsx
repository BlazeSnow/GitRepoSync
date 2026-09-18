import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "@/lib/api";
import type { AvailableRepo, Repo, SyncEvent, SyncStatus } from "@/lib/types";
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
import { IconPlus, IconRefresh, IconSquare } from "@/components/icons";

const STALE_DAYS = [1, 3, 7, 30];

export function SyncPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [repos, setRepos] = useState<Repo[]>([]);
  const [stale, setStale] = useState("all");
  const [edit, setEdit] = useState<EditState | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<Repo | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; repo: Repo } | null>(null);
  const [error, setError] = useState("");
  const [available, setAvailable] = useState<AvailableRepo[] | null>(null);
  const [availablePlatform, setAvailablePlatform] = useState<string>("");

  const load = useCallback(async () => {
    try {
      setRepos(await api.listRepos(token));
      setError("");
    } catch (err) {
      setError(String(err));
    }
  }, [token]);

  const loadAvailable = useCallback(async () => {
    try {
      const platform = await api.getPrimaryPlatform(token);
      if (!platform) {
        setAvailable(null);
        return;
      }
      setAvailablePlatform(platform);
      setAvailable(await api.listAvailableRepos(token));
    } catch {
      setAvailable(null);
    }
  }, [token]);

  useEffect(() => {
    void load();
    void loadAvailable();
    const unlisten = listen<SyncEvent>("sync-status", () => {
      void load();
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [load, loadAvailable]);

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
        target: edit.target,
      });
      setEdit(null);
      void load();
      void loadAvailable();
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
      void loadAvailable();
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
          onSelect: () =>
            setEdit({
              id: menu.repo.id,
              name: menu.repo.name,
              source: menu.repo.source,
              target: menu.repo.target,
            }),
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
        <Button
          variant="secondary"
          onClick={() => setEdit({ id: null, name: "", source: "", target: "" })}
        >
          <IconPlus />
          {t("addRepo")}
        </Button>
      </div>

      {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

      <div className="min-h-0 flex-1 rounded-lg border bg-card">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-52">{t("colRepo")}</TableHead>
              <TableHead>{t("colSource")}</TableHead>
              <TableHead>{t("colTarget")}</TableHead>
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
                    onDoubleClick={() =>
                      setEdit({
                        id: repo.id,
                        name: repo.name,
                        source: repo.source,
                        target: repo.target,
                      })
                    }
                    onContextMenu={(e) => openMenu(e, repo)}
                    title={repo.lastMessage ?? undefined}
                  >
                    <TableCell className="font-medium">{repo.name}</TableCell>
                    <TableCell
                      className="max-w-0 truncate text-muted-foreground"
                      title={repo.source}
                    >
                      {repo.source}
                    </TableCell>
                    <TableCell
                      className="max-w-0 truncate text-muted-foreground"
                      title={repo.target}
                    >
                      {repo.target}
                    </TableCell>
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

      {available !== null && (
        <div className="mt-4 rounded-lg border bg-card p-4">
          <div className="mb-3 flex items-center gap-3">
            <h2 className="text-sm font-semibold">{t("availableTitle")}</h2>
            {availablePlatform && (
              <span className="text-xs text-muted-foreground">
                {t("availableFrom", { platform: availablePlatform })}
              </span>
            )}
            <div className="flex-1" />
            <Button variant="outline" size="sm" onClick={() => void loadAvailable()}>
              <IconRefresh />
              {t("refresh")}
            </Button>
          </div>
          {available.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("availableEmpty")}</p>
          ) : (
            <div className="space-y-1.5">
              {available.map((r) => (
                <div
                  key={`${r.platform}-${r.platformId}`}
                  className="flex items-center gap-3 rounded-md border px-3 py-2"
                >
                  <div className="min-w-0 flex-1 leading-tight">
                    <div className="truncate text-sm font-medium">{r.fullName}</div>
                    <div className="truncate text-xs text-muted-foreground">{r.cloneUrl}</div>
                  </div>
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() =>
                      setEdit({ id: null, name: r.name, source: r.cloneUrl, target: "" })
                    }
                  >
                    <IconPlus />
                    {t("quickAdd")}
                  </Button>
                </div>
              ))}
            </div>
          )}
          <p className="mt-3 text-xs text-muted-foreground">{t("availableHint")}</p>
        </div>
      )}

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
              <Label htmlFor="repo-target">{t("fieldTarget")}</Label>
              <Input
                id="repo-target"
                value={edit?.target ?? ""}
                onChange={(e) => setEdit((s) => (s ? { ...s, target: e.target.value } : s))}
                placeholder={t("placeholderTarget")}
              />
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

interface EditState {
  id: string | null;
  name: string;
  source: string;
  target: string;
}
