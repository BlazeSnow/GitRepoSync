import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AppInfo, McpConfig, Repo } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { IconCopy, IconRefresh } from "@/components/icons";

export function SettingsPage({ token, username }: { token: string; username: string }) {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [mcp, setMcp] = useState<McpConfig | null>(null);
  const [repos, setRepos] = useState<Repo[]>([]);
  const [oldPwd, setOldPwd] = useState("");
  const [newPwd, setNewPwd] = useState("");
  const [confirmPwd, setConfirmPwd] = useState("");
  const [pwdMsg, setPwdMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [copied, setCopied] = useState("");

  useEffect(() => {
    api.getAppInfo(token).then(setAppInfo).catch(() => {});
    api.getMcpConfig(token).then(setMcp).catch(() => {});
    api.listRepos(token).then(setRepos).catch(() => {});
  }, [token]);

  const mcpExample = mcp
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

  async function handleChangePassword() {
    setPwdMsg(null);
    if (newPwd !== confirmPwd) {
      setPwdMsg({ text: "两次输入的新密码不一致", ok: false });
      return;
    }
    try {
      await api.changePassword(token, oldPwd, newPwd);
      setPwdMsg({ text: "密码修改成功", ok: true });
      setOldPwd("");
      setNewPwd("");
      setConfirmPwd("");
    } catch (err) {
      setPwdMsg({ text: String(err), ok: false });
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
      <h1 className="text-lg font-semibold">设置</h1>

      <Card>
        <CardHeader>
          <CardTitle>账户</CardTitle>
          <CardDescription>当前用户：{username}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-3">
            <div className="space-y-1.5">
              <Label htmlFor="old-pwd">旧密码</Label>
              <Input id="old-pwd" type="password" value={oldPwd} onChange={(e) => setOldPwd(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="new-pwd">新密码</Label>
              <Input id="new-pwd" type="password" value={newPwd} onChange={(e) => setNewPwd(e.target.value)} />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="confirm-pwd">确认新密码</Label>
              <Input
                id="confirm-pwd"
                type="password"
                value={confirmPwd}
                onChange={(e) => setConfirmPwd(e.target.value)}
              />
            </div>
          </div>
          <div className="flex items-center gap-3">
            <Button size="sm" onClick={handleChangePassword} disabled={!oldPwd || !newPwd}>
              修改密码
            </Button>
            {pwdMsg && (
              <span className={`text-xs ${pwdMsg.ok ? "text-success" : "text-destructive"}`}>{pwdMsg.text}</span>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Agent（MCP 接入）</CardTitle>
          <CardDescription>
            Agent 通过 MCP stdio 方式连接本软件（APIKEY 鉴权），可管理仓库并触发同步
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="space-y-1.5">
            <Label>API Key</Label>
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
            <Label>MCP 客户端配置示例</Label>
            <div className="relative">
              <pre className="overflow-x-auto rounded-md border bg-muted/50 p-3 pr-12 font-mono text-xs leading-relaxed">
                {mcpExample || "…"}
              </pre>
              <Button
                variant="outline"
                size="icon"
                className="absolute right-2 top-2"
                title="复制配置"
                onClick={() => copyText(mcpExample, "config")}
              >
                <IconCopy />
              </Button>
            </div>
            {copied === "config" && <p className="text-xs text-success">已复制到剪贴板</p>}
            <p className="text-xs text-muted-foreground">
              将以上配置加入 MCP 客户端后，Agent 以子进程方式运行本程序的 mcp 模式（stdio 通信）。
            </p>
          </div>

          <div className="text-xs text-muted-foreground">
            可用工具：list_repos、add_repo、remove_repo、sync_repo、get_sync_status
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>软件信息</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1 text-sm">
          <div className="flex gap-2">
            <span className="w-24 text-muted-foreground">软件名称</span>
            <span>{appInfo?.name ?? "Git Repo Sync"}</span>
          </div>
          <div className="flex gap-2">
            <span className="w-24 text-muted-foreground">版本号</span>
            <Badge variant="secondary">v{appInfo?.version ?? "…"}</Badge>
          </div>
          <div className="flex gap-2">
            <span className="w-24 text-muted-foreground">操作系统</span>
            <span>{appInfo?.os ?? "…"}</span>
          </div>
          <div className="flex gap-2">
            <span className="w-24 text-muted-foreground">本软件仓库</span>
            <span>{repos.length} 个</span>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>仓库列表</CardTitle>
          <CardDescription>本软件管理的全部同步仓库（只读，可在“同步仓库”页面编辑）</CardDescription>
        </CardHeader>
        <CardContent>
          {repos.length === 0 ? (
            <p className="text-sm text-muted-foreground">暂无仓库</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead>仓库</TableHead>
                  <TableHead>源地址</TableHead>
                  <TableHead>目标地址</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {repos.map((r) => (
                  <TableRow key={r.id}>
                    <TableCell className="font-medium">{r.name}</TableCell>
                    <TableCell className="max-w-0 truncate text-muted-foreground" title={r.source}>
                      {r.source}
                    </TableCell>
                    <TableCell className="max-w-0 truncate text-muted-foreground" title={r.target}>
                      {r.target}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
