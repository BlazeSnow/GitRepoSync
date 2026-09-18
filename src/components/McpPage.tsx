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

const MCP_TOOLS: { name: string; description: string }[] = [
  { name: "list_repos", description: "列出所有已配置的同步仓库及其最近一次同步状态" },
  {
    name: "add_repo",
    description: "新增同步仓库（name / source / target），从源仓库同步到目标仓库",
  },
  { name: "remove_repo", description: "删除指定的同步仓库" },
  { name: "sync_repo", description: "立即开始同步指定仓库（异步执行）" },
  { name: "get_sync_status", description: "查询所有仓库的最近同步状态" },
  { name: "get_base_dir", description: "查询本地仓库基地址（中转站目录）" },
  { name: "set_base_dir", description: "修改本地仓库基地址（中转站目录）" },
];

export function McpPage({ token }: { token: string }) {
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
      <h1 className="text-lg font-semibold">MCP</h1>

      <Card>
        <CardHeader>
          <CardTitle>连接配置</CardTitle>
          <CardDescription>
            Agent 通过 MCP stdio 方式连接本软件（APIKEY 鉴权），可管理仓库并触发同步
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1.5">
            <div className="text-sm font-medium">API Key</div>
            <div className="flex items-center gap-2">
              <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
                {mcp?.apiKey ?? "…"}
              </code>
              <Button
                variant="outline"
                size="icon"
                title="复制"
                onClick={() => mcp && copyText(mcp.apiKey, "key")}
              >
                <IconCopy />
              </Button>
              <Button variant="outline" size="icon" title="重新生成" onClick={handleRegenerateKey}>
                <IconRefresh />
              </Button>
            </div>
            {copied === "key" && <p className="text-xs text-success">已复制到剪贴板</p>}
          </div>

          <div className="space-y-1.5">
            <div className="text-sm font-medium">MCP 客户端配置示例</div>
            <div className="relative">
              <pre className="overflow-x-auto rounded-md border bg-muted/50 p-3 pr-12 font-mono text-xs leading-relaxed">
                {example || "…"}
              </pre>
              <Button
                variant="outline"
                size="icon"
                className="absolute right-2 top-2"
                title="复制配置"
                onClick={() => copyText(example, "config")}
              >
                <IconCopy />
              </Button>
            </div>
            {copied === "config" && <p className="text-xs text-success">已复制到剪贴板</p>}
            <p className="text-xs text-muted-foreground">
              将以上配置加入 MCP 客户端后，Agent 以子进程方式运行本程序的 mcp 模式（stdio 通信）。
            </p>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>可用工具</CardTitle>
          <CardDescription>MCP 提供以下工具供 Agent 调用</CardDescription>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow className="hover:bg-transparent">
                <TableHead className="w-44">工具名</TableHead>
                <TableHead>说明</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {MCP_TOOLS.map((tool) => (
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
