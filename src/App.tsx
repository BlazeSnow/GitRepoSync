import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import { Layout, type PageKey } from "@/components/Layout";
import { Login } from "@/components/Login";
import { ProvidersPage } from "@/components/ProvidersPage";
import { SettingsPage } from "@/components/SettingsPage";
import { SyncPage } from "@/components/SyncPage";

const TOKEN_KEY = "grs_token";

export default function App() {
  const [token, setToken] = useState<string | null>(() => localStorage.getItem(TOKEN_KEY));
  const [username, setUsername] = useState("");
  const [checking, setChecking] = useState(() => !!localStorage.getItem(TOKEN_KEY));
  const [page, setPage] = useState<PageKey>("sync");
  const [version, setVersion] = useState("…");

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
        正在恢复登录…
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
      {page === "providers" && <ProvidersPage token={token} />}
      {page === "settings" && <SettingsPage token={token} username={username} />}
    </Layout>
  );
}
