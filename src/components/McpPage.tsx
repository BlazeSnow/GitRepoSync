import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { McpConfig } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { IconCopy, IconRefresh } from "@/components/icons";
import { useTranslation } from "react-i18next";

export function McpPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [mcp, setMcp] = useState<McpConfig | null>(null);
  const [copied, setCopied] = useState("");

  useEffect(() => {
    api.getMcpConfig(token).then(setMcp).catch(() => {});
  }, [token]);

  const example = mcp
    ? JSON.stringify(
        {
          mcpServers: {
            "git-repo-sync": {
              command: mcp.exePath,
              args: ["mcp"],
              env: { GIT_REPO_SYNC_API_KEY: mcp.apiKey },
            },
          },
        },
        null,
        2,
      )
    : "";

  const tools: { name: string; description: string }[] = [
    { name: "list_repos", description: t("toolListRepos") },
    { name: "add_repo", description: t("toolAddRepo") },
    { name: "remove_repo", description: t("toolRemoveRepo") },
    { name: "sync_repo", description: t("toolSyncRepo") },
    { name: "get_sync_status", description: t("toolGetSyncStatus") },
    { name: "get_base_dir", description: t("toolGetBaseDir") },
    { name: "set_base_dir", description: t("toolSetBaseDir") },
  ];

  async function copyText(text: string, marker: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(marker);
      setTimeout(() => setCopied(""), 1500);
    } catch {
      /* 剪贴板不可用 */
    }
  }

  async function handleRegenerateKey() {
    try {
      const key = await api.regenerateMcpKey(token);
      setMcp((c) => (c ? { ...c, apiKey: key } : c));
    } catch {
      /* 忽略 */
    }
  }

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-6">
      <h1 className="text-lg font-semibold">{t("mcpTitle")}</h1>

      <Card>
        <CardHeader>
          <CardTitle>{t("mcpConnConfig")}</CardTitle>
          <CardDescription>{t("mcpConnDesc")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1.5">
            <div className="text-sm font-medium">{t("mcpApiKey")}</div>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
                {mcp?.apiKey ?? "…"}
              </code>
              <Button
                variant="outline"
                size="icon"
                title={t("copy")}
                onClick={() => mcp && copyText(mcp.apiKey, "key")}
              >
                <IconCopy />
              </Button>
              <Button variant="outline" size="icon" title={t("refresh")} onClick={handleRegenerateKey}>
                <IconRefresh />
              </Button>
            </div>
            {copied === "key" && <p className="text-xs text-success">{t("copied")}</p>}
          </div>

          <div className="space-y-1.5">
            <div className="text-sm font-medium">{t("mcpExample")}</div>
            <div className="relative">
              <pre className="overflow-x-auto rounded-md border bg-muted/50 p-3 pr-12 font-mono text-xs leading-relaxed">
                {example || "…"}
              </pre>
              <Button
                variant="outline"
                size="icon"
                className="absolute right-2 top-2"
                title={t("copy")}
                onClick={() => copyText(example, "config")}
              >
                <IconCopy />
              </Button>
            </div>
            {copied === "config" && <p className="text-xs text-success">{t("copied")}</p>}
            <p className="text-xs text-muted-foreground">{t("mcpExampleDesc")}</p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("mcpTools")}</CardTitle>
          <CardDescription>{t("mcpToolsDesc")}</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead className="w-44">{t("mcpColTool")}</TableHead>
                <TableHead>{t("mcpColDesc")}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {tools.map((tool) => (
                <TableRow key={tool.name}>
                  <TableCell className="font-mono text-xs font-medium">{tool.name}</TableCell>
                  <TableCell className="text-muted-foreground">{tool.description}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
    </div>
  );
}
