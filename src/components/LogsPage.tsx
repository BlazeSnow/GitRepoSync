import { useCallback, useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { OperationLog } from "@/lib/types";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { IconRefresh } from "@/components/icons";
import { useTranslation } from "react-i18next";

function formatDateTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(
    d.getMinutes(),
  )}:${p(d.getSeconds())}`;
}

export function LogsPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [logs, setLogs] = useState<OperationLog[]>([]);
  const [error, setError] = useState("");

  const load = useCallback(async () => {
    try {
      setLogs(await api.listLogs(token, 500));
      setError("");
    } catch (err) {
      setError(String(err));
    }
  }, [token]);

  useEffect(() => {
    void load();
  }, [load]);

  return (
    <div className="flex h-full flex-col p-6">
      <div className="mb-4 flex items-center gap-3">
        <h1 className="text-lg font-semibold">{t("logsTitle")}</h1>
        <span className="text-xs text-muted-foreground">{t("logsRecent")}</span>
        <div className="flex-1" />
        <Button variant="outline" size="sm" onClick={() => void load()}>
          <IconRefresh />
          {t("refresh")}
        </Button>
      </div>

      {error && <p className="mb-3 text-sm text-destructive">{error}</p>}

      <div className="min-h-0 flex-1 rounded-lg border bg-card">
        <Table>
          <TableHeader>
            <TableRow className="hover:bg-transparent">
              <TableHead className="w-56">{t("colTime")}</TableHead>
              <TableHead>{t("colAction")}</TableHead>
              <TableHead className="w-40">{t("colOperator")}</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {logs.length === 0 ? (
              <TableRow>
                <TableCell colSpan={3} className="h-32 text-center text-muted-foreground">
                  {t("logsEmpty")}
                </TableCell>
              </TableRow>
            ) : (
              logs.map((log) => (
                <TableRow key={log.id}>
                  <TableCell className="whitespace-nowrap text-muted-foreground">
                    {formatDateTime(log.createdAt)}
                  </TableCell>
                  <TableCell>{log.action}</TableCell>
                  <TableCell>{log.operator}</TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>
    </div>
  );
}
