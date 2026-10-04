import { useTranslation } from "react-i18next";
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
import { Label } from "@/components/ui/label";

/** 仓库详情弹窗（只读）：仓库配置由 git remote 管理，软件不代管 */
export function RepoDetailDialog({ repo, onClose }: { repo: Repo; onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Dialog open onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        {/* min-w-0：弹窗为 grid 容器，网格子项默认 min-width:auto 会让
            不可断行的长 URL 把轨道撑出弹窗边框，truncate 因此失效 */}
        <DialogHeader className="min-w-0">
          <DialogTitle className="truncate">{repo.name}</DialogTitle>
          <DialogDescription>{t("detailReadOnlyDesc")}</DialogDescription>
        </DialogHeader>
        <div className="min-w-0 space-y-4">
          <div className="space-y-1.5">
            <Label>origin（{t("fieldSource")}）</Label>
            <code className="block truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
              {repo.source || t("notConfigured")}
            </code>
          </div>
          <div className="space-y-1.5">
            <Label>{t("fieldTarget")}</Label>
            {repo.targets.length === 0 ? (
              <p className="text-sm text-muted-foreground">{t("notConfigured")}</p>
            ) : (
              <div className="space-y-1.5">
                {repo.targets.map((tg) => (
                  <div key={tg.remote} className="flex items-center gap-2">
                    <code className="w-28 shrink-0 truncate rounded-md border bg-muted/50 px-2 py-2 font-mono text-xs">
                      {tg.remote}
                    </code>
                    <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
                      {tg.url}
                    </code>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={onClose}>
            {t("close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
