import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import type { Repo } from "@/lib/types";
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
import { IconPlus, IconX } from "@/components/icons";

/** 仓库编辑弹窗（repo 为 null 表示添加）；名称/源地址/目标列表的编辑状态由弹窗内部持有 */
export function RepoEditDialog({
  token,
  repo,
  onClose,
  onSaved,
}: {
  token: string;
  repo: Repo | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(repo?.name ?? "");
  const [source, setSource] = useState(repo?.source ?? "");
  const [targets, setTargets] = useState<{ remote: string; url: string }[]>(
    repo
      ? repo.targets.map((tg) => ({ remote: tg.remote, url: tg.url }))
      : [{ remote: "backup", url: "" }],
  );
  const [error, setError] = useState("");

  async function handleSave() {
    setError("");
    try {
      await api.saveRepo(token, {
        id: repo?.id ?? null,
        name,
        source,
        targets: targets.filter((tg) => tg.remote.trim() || tg.url.trim()),
      });
      onSaved();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{repo ? t("editRepo") : t("addRepoTitle")}</DialogTitle>
          <DialogDescription>{t("repoFlowDesc")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-4">
          <div className="space-y-1.5">
            <Label htmlFor="repo-name">{t("fieldName")}</Label>
            <Input
              id="repo-name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("placeholderName")}
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="repo-source">{t("fieldSource")}</Label>
            <Input
              id="repo-source"
              value={source}
              onChange={(e) => setSource(e.target.value)}
              placeholder={t("placeholderSource")}
            />
          </div>
          <div className="space-y-1.5">
            <Label>{t("fieldTarget")}</Label>
            <div className="space-y-1.5">
              {targets.map((tg, i) => (
                <div key={i} className="flex items-center gap-2">
                  <Input
                    className="w-28 shrink-0"
                    placeholder={t("targetRemote")}
                    value={tg.remote}
                    onChange={(e) =>
                      setTargets((ts) =>
                        ts.map((x, j) => (j === i ? { ...x, remote: e.target.value } : x)),
                      )
                    }
                  />
                  <Input
                    className="min-w-0 flex-1 font-mono text-xs"
                    placeholder={t("targetUrl")}
                    value={tg.url}
                    onChange={(e) =>
                      setTargets((ts) =>
                        ts.map((x, j) => (j === i ? { ...x, url: e.target.value } : x)),
                      )
                    }
                  />
                  <Button
                    variant="ghost"
                    size="icon"
                    title={t("confirmDelete")}
                    onClick={() => setTargets((ts) => ts.filter((_, j) => j !== i))}
                  >
                    <IconX />
                  </Button>
                </div>
              ))}
              <Button
                variant="outline"
                size="sm"
                onClick={() => setTargets((ts) => [...ts, { remote: "", url: "" }])}
              >
                <IconPlus />
                {t("addTarget")}
              </Button>
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("cancel")}
          </Button>
          <Button onClick={() => void handleSave()}>{t("save")}</Button>
        </DialogFooter>
        {error && <p className="text-sm text-destructive">{error}</p>}
      </DialogContent>
    </Dialog>
  );
}
