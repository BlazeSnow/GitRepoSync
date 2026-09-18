import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AppInfo, Repo } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";

export function SettingsPage({ token, username }: { token: string; username: string }) {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [repos, setRepos] = useState<Repo[]>([]);
  const [baseDir, setBaseDir] = useState("");
  const [baseDirInput, setBaseDirInput] = useState("");
  const [oldPwd, setOldPwd] = useState("");
  const [newPwd, setNewPwd] = useState("");
  const [confirmPwd, setConfirmPwd] = useState("");
  const [pwdMsg, setPwdMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [baseMsg, setBaseMsg] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => {
    api.getAppInfo(token).then(setAppInfo).catch(() => {});
    api.listRepos(token).then(setRepos).catch(() => {});
    api
      .getBaseDir(token)
      .then((dir) => {
        setBaseDir(dir);
        setBaseDirInput(dir);
      })
      .catch(() => {});
  }, [token]);

  async function handleSaveBaseDir() {
    setBaseMsg(null);
    try {
      await api.setBaseDir(token, baseDirInput);
      setBaseDir(baseDirInput.trim());
      setBaseMsg({ text: "基地址已保存", ok: true });
    } catch (err) {
      setBaseMsg({ text: String(err), ok: false });
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

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-6">
      <h1 className="text-lg font-semibold">设置</h1>

      <Card>
        <CardHeader>
          <CardTitle>仓库基地址</CardTitle>
          <CardDescription>
            同步时从源仓库拉取到基地址下的本地中转目录（按仓库名建目录），更新 LFS 与
            submodule 后推送到目标仓库。支持 ~ 开头的路径。
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-3">
            <Input
              className="max-w-sm font-mono text-xs"
              value={baseDirInput}
              onChange={(e) => setBaseDirInput(e.target.value)}
              placeholder="~/repo"
            />
            <Button
              size="sm"
              variant="secondary"
              onClick={handleSaveBaseDir}
              disabled={!baseDirInput.trim() || baseDirInput === baseDir}
            >
              保存
            </Button>
            {baseMsg && (
              <span className={`text-xs ${baseMsg.ok ? "text-success" : "text-destructive"}`}>
                {baseMsg.text}
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">
            当前基地址：{baseDir || "…"}
          </p>
        </CardContent>
      </Card>

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
