import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AppInfo, Repo } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { useTranslation } from "react-i18next";

export function SettingsPage({ token, username }: { token: string; username: string }) {
  const { t } = useTranslation();
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
      setBaseMsg({ text: t("baseDirSaved"), ok: true });
    } catch (err) {
      setBaseMsg({ text: String(err), ok: false });
    }
  }

  async function handleChangePassword() {
    setPwdMsg(null);
    if (newPwd !== confirmPwd) {
      setPwdMsg({ text: t("pwdMismatch"), ok: false });
      return;
    }
    try {
      await api.changePassword(token, oldPwd, newPwd);
      setPwdMsg({ text: t("pwdChanged"), ok: true });
      setOldPwd("");
      setNewPwd("");
      setConfirmPwd("");
    } catch (err) {
      setPwdMsg({ text: String(err), ok: false });
    }
  }

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-6">
      <h1 className="text-lg font-semibold">{t("settingsTitle")}</h1>

      <Card>
        <CardHeader>
          <CardTitle>{t("baseDirTitle")}</CardTitle>
          <CardDescription>{t("baseDirDesc")}</CardDescription>
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
              {t("save")}
            </Button>
            {baseMsg && (
              <span className={`text-xs ${baseMsg.ok ? "text-success" : "text-destructive"}`}>
                {baseMsg.text}
              </span>
            )}
          </div>
          <p className="text-xs text-muted-foreground">{t("currentBaseDir", { dir: baseDir || "…" })}</p>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("account")}</CardTitle>
          <CardDescription>{t("currentUser", { user: username })}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid gap-4 sm:grid-cols-3">
            <div className="space-y-1.5">
              <Label htmlFor="old-pwd">{t("oldPassword")}</Label>
              <Input
                id="old-pwd"
                type="password"
                value={oldPwd}
                onChange={(e) => setOldPwd(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="new-pwd">{t("newPassword")}</Label>
              <Input
                id="new-pwd"
                type="password"
                value={newPwd}
                onChange={(e) => setNewPwd(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="confirm-pwd">{t("confirmPassword")}</Label>
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
              {t("changePassword")}
            </Button>
            {pwdMsg && (
              <span className={`text-xs ${pwdMsg.ok ? "text-success" : "text-destructive"}`}>
                {pwdMsg.text}
              </span>
            )}
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("softwareInfo")}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-1 text-sm">
          <div className="flex gap-2">
            <span className="w-32 text-muted-foreground">{t("appName")}</span>
            <span>{appInfo?.name ?? "Git Repo Sync"}</span>
          </div>
          <div className="flex gap-2">
            <span className="w-32 text-muted-foreground">{t("versionLabel")}</span>
            <Badge variant="secondary">v{appInfo?.version ?? "…"}</Badge>
          </div>
          <div className="flex gap-2">
            <span className="w-32 text-muted-foreground">{t("osLabel")}</span>
            <span>{appInfo?.os ?? "…"}</span>
          </div>
          <div className="flex gap-2">
            <span className="w-32 text-muted-foreground">{t("repoCountLabel")}</span>
            <span>{repos.length}</span>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("repoListTitle")}</CardTitle>
          <CardDescription>{t("repoListDesc")}</CardDescription>
        </CardHeader>
        <CardContent>
          {repos.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("listEmpty")}</p>
          ) : (
            <Table>
              <TableHeader>
                <TableRow className="hover:bg-transparent">
                  <TableHead>{t("colRepo")}</TableHead>
                  <TableHead>{t("colSource")}</TableHead>
                  <TableHead>{t("colTarget")}</TableHead>
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
