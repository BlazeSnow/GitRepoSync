import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AppInfo } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useTranslation } from "react-i18next";
import { changeAppLang } from "@/i18n";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";

/** 本软件的源码仓库 */
const SOURCE_REPO_URL = "https://github.com/BlazeSnow/GitRepoSync";

export function SettingsPage({ token, username }: { token: string; username: string }) {
  const { t, i18n } = useTranslation();
  const isZh = i18n.language.toLowerCase().startsWith("zh");
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [baseDir, setBaseDir] = useState("");
  const [baseDirInput, setBaseDirInput] = useState("");
  const [oldPwd, setOldPwd] = useState("");
  const [newPwd, setNewPwd] = useState("");
  const [confirmPwd, setConfirmPwd] = useState("");
  const [pwdMsg, setPwdMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [baseMsg, setBaseMsg] = useState<{ text: string; ok: boolean } | null>(null);

  useEffect(() => {
    api.getAppInfo(token).then(setAppInfo).catch(() => {});
    api
      .getBaseDir(token)
      .then((dir) => {
        setBaseDir(dir);
        setBaseDirInput(dir);
      })
      .catch(() => {});
  }, [token]);

  async function browseBaseDir() {
    try {
      const dir = await open({ directory: true, multiple: false });
      if (typeof dir === "string" && dir) {
        setBaseDirInput(dir);
      }
    } catch {
      /* 用户取消或对话框不可用 */
    }
  }

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
          <CardTitle>语言 / Language</CardTitle>
          <CardDescription>{t("langDesc")}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex gap-2">
            <Button variant={isZh ? "default" : "outline"} onClick={() => void changeAppLang("zh")}>
              中文
            </Button>
            <Button variant={!isZh ? "default" : "outline"} onClick={() => void changeAppLang("en")}>
              English
            </Button>
          </div>
        </CardContent>
      </Card>

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
            <Button variant="outline" onClick={() => void browseBaseDir()}>
              {t("browse")}
            </Button>
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
          <p className="text-xs text-muted-foreground">
            {t("currentBaseDir", { dir: baseDir || "…" })}
          </p>
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
          <div className="flex items-center gap-2">
            <span className="w-32 text-muted-foreground">{t("versionLabel")}</span>
            <Badge variant="secondary">v{appInfo?.version ?? "…"}</Badge>
          </div>
          <div className="flex items-center gap-2">
            <span className="w-32 shrink-0 text-muted-foreground">{t("repoLinkLabel")}</span>
            <button
              type="button"
              title={t("openRepoLink")}
              className="truncate font-mono text-xs text-primary underline-offset-2 hover:underline"
              onClick={() => void openUrl(SOURCE_REPO_URL).catch(() => {})}
            >
              {SOURCE_REPO_URL}
            </button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}
