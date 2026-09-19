import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { PageKey } from "@/lib/types";
import { applyTheme, getThemePref, watchSystemTheme } from "@/lib/theme";
import { Layout } from "@/components/Layout";
import { Login } from "@/components/Login";
import { LogsPage } from "@/components/LogsPage";
import { McpPage } from "@/components/McpPage";
import { SettingsPage } from "@/components/SettingsPage";
import { SyncPage } from "@/components/SyncPage";
import { useTranslation } from "react-i18next";

const TOKEN_KEY = "grs_token";

export default function App() {
  const { t } = useTranslation();
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(TOKEN_KEY));
  const [username, setUsername] = useState("");
  const [checking, setChecking] = useState(() => !!localStorage.getItem(TOKEN_KEY));
  const [page, setPage] = useState<PageKey>("sync");
  const [version, setVersion] = useState("…");

  // 应用主题偏好并监听系统深浅色变化（跟随系统时即时切换）
  useEffect(() => {
    applyTheme(getThemePref());
    return watchSystemTheme();
  }, []);

  useEffect(() => {
    if (!token) return;
    let cancelled = false;
    api
      .restoreSession(token)
      .then((user) => {
        if (cancelled) return;
        setUsername(user);
        setChecking(false);
        api.getAppInfo(token).then((info) => setVersion(info.version)).catch(() => {});
      })
      .catch(() => {
        if (cancelled) return;
        localStorage.removeItem(TOKEN_KEY);
        setToken(null);
        setChecking(false);
      });
    return () => {
      cancelled = true;
    };
  }, [token]);

  async function handleLogout() {
    if (token) {
      try {
        await api.logout(token);
      } catch {
        /* 忽略 */
      }
    }
    localStorage.removeItem(TOKEN_KEY);
    setToken(null);
    setUsername("");
    setPage("sync");
  }

  if (checking) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t("restoring")}
      </div>
    );
  }

  if (!token) {
    return (
    <Login
      onLogin={(t, user) => {
        localStorage.setItem(TOKEN_KEY, t);
        setToken(t);
        setUsername(user);
        api.getAppInfo(t).then((info) => setVersion(info.version)).catch(() => {});
      }}
    />
    );
  }

  return (
    <Layout
      page={page}
      onNavigate={setPage}
      username={username}
      version={version}
      onLogout={handleLogout}
    >
      {page === "sync" && <SyncPage token={token} />}
      {page === "logs" && <LogsPage token={token} />}
      {page === "mcp" && <McpPage token={token} />}
      {page === "settings" && <SettingsPage token={token} username={username} />}
    </Layout>
  );
}
