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
import { getThemePref, setThemePref, type ThemePref } from "@/lib/theme";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open } from "@tauri-apps/plugin-dialog";

/** 本软件的源码仓库 */
const SOURCE_REPO_URL = "https://github.com/BlazeSnow/GitRepoSync";

export function SettingsPage({ token, username }: { token: string; username: string }) {
  const { t, i18n } = useTranslation();
  const isZh = i18n.language.toLowerCase().startsWith("zh");
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null);
  const [baseDir, setBaseDir] = useState("");
  const [baseDirLoading, setBaseDirLoading] = useState(false);
  const [oldPwd, setOldPwd] = useState("");
  const [newPwd, setNewPwd] = useState("");
  const [confirmPwd, setConfirmPwd] = useState("");
  const [pwdMsg, setPwdMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [baseMsg, setBaseMsg] = useState<{ text: string; ok: boolean } | null>(null);
  const [theme, setTheme] = useState<ThemePref>(() => getThemePref());

  function pickTheme(pref: ThemePref) {
    setTheme(pref);
    setThemePref(pref);
  }

  useEffect(() => {
    api.getAppInfo(token).then(setAppInfo).catch(() => {});
    api
      .getBaseDir(token)
      .then(setBaseDir)
      .catch(() => {});
  }, [token]);

  async function browseBaseDir() {
    try {
      const dir = await open({ directory: true, multiple: false, defaultPath: baseDir || undefined });
      if (typeof dir === "string" && dir) {
        setBaseDirLoading(true);
        setBaseMsg(null);
        try {
          await api.setBaseDir(token, dir);
          setBaseDir(dir);
          setBaseMsg({ text: t("baseDirSaved"), ok: true });
        } catch (err) {
          setBaseMsg({ text: String(err), ok: false });
        } finally {
          setBaseDirLoading(false);
        }
      }
    } catch {
      /* 用户取消或对话框不可用 */
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
          <CardTitle>{t("appearanceTitle")}</CardTitle>
          <CardDescription>{t("themeDesc")}</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="flex gap-2">
            {(
              [
                ["system", t("themeSystem")],
                ["light", t("themeLight")],
                ["dark", t("themeDark")],
              ] as [ThemePref, string][]
            ).map(([pref, label]) => (
              <Button
                key={pref}
                variant={theme === pref ? "default" : "outline"}
                onClick={() => pickTheme(pref)}
              >
                {label}
              </Button>
            ))}
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
            <code className="min-w-0 flex-1 truncate rounded-md border bg-muted/50 px-3 py-2 font-mono text-xs">
              {baseDir || "…"}
            </code>
            <Button variant="outline" disabled={baseDirLoading} onClick={() => void browseBaseDir()}>
              {t("browse")}
            </Button>
          </div>
          {baseMsg && (
            <p className={`text-xs ${baseMsg.ok ? "text-success" : "text-destructive"}`}>
              {baseMsg.text}
            </p>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>{t("account")}</CardTitle>
          <CardDescription>{t("currentUser", { user: username })}</CardDescription>
        </CardHeader>
        <CardContent>
          {/* 真实 form 语义 + autocomplete 标注：密码管理器借此识别「修改密码」场景
              （readonly 用户名字段用于关联凭据，current/new-password 区分新旧密码） */}
          <form
            className="space-y-4"
            onSubmit={(e) => {
              e.preventDefault();
              void handleChangePassword();
            }}
          >
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="pwd-username">{t("username")}</Label>
                <Input
                  id="pwd-username"
                  type="text"
                  value={username}
                  readOnly
                  autoComplete="username"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="old-pwd">{t("oldPassword")}</Label>
                <Input
                  id="old-pwd"
                  type="password"
                  value={oldPwd}
                  onChange={(e) => setOldPwd(e.target.value)}
                  autoComplete="current-password"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="new-pwd">{t("newPassword")}</Label>
                <Input
                  id="new-pwd"
                  type="password"
                  value={newPwd}
                  onChange={(e) => setNewPwd(e.target.value)}
                  autoComplete="new-password"
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="confirm-pwd">{t("confirmPassword")}</Label>
                <Input
                  id="confirm-pwd"
                  type="password"
                  value={confirmPwd}
                  onChange={(e) => setConfirmPwd(e.target.value)}
                  autoComplete="new-password"
                />
              </div>
            </div>
            <div className="flex items-center gap-3">
              <Button type="submit" size="sm" disabled={!oldPwd || !newPwd}>
                {t("changePassword")}
              </Button>
              {pwdMsg && (
                <span className={`text-xs ${pwdMsg.ok ? "text-success" : "text-destructive"}`}>
                  {pwdMsg.text}
                </span>
              )}
            </div>
          </form>
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
