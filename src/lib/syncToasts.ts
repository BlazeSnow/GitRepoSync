// 同步状态事件 → toast：GUI 全局订阅 sync-status，把同步结果实时弹出
// （成功 / 失败附按目标完整原因 / 停止）；「同步中」为过程态不提示。
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { useToast } from "@/components/Toast";
import type { SyncEvent } from "./types";

export function useSyncToasts(): void {
  const toast = useToast();
  const { t } = useTranslation();
  useEffect(() => {
    const unlisten = listen<SyncEvent>("sync-status", (e) => {
      const ev = e.payload;
      if (ev.status === "success") {
        toast({ kind: "success", title: t("toastSyncSuccess", { name: ev.name }) });
      } else if (ev.status === "failed") {
        toast({
          kind: "error",
          title: t("toastSyncFailed", { name: ev.name }),
          description: ev.message ?? undefined,
        });
      } else if (ev.status === "stopped") {
        toast({ kind: "info", title: t("toastSyncStopped", { name: ev.name }) });
      }
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [toast, t]);
}
