import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { LangButton, useI18n } from "@/i18n";
import {
  IconBranch,
  IconCloud,
  IconList,
  IconLogout,
  IconPlug,
  IconSettings,
  Logo,
} from "@/components/icons";
import type { PageKey } from "@/lib/types";

export function Layout({
  page,
  onNavigate,
  username,
  version,
  onLogout,
  children,
}: {
  page: PageKey;
  onNavigate: (p: PageKey) => void;
  username: string;
  version: string;
  onLogout: () => void;
  children: ReactNode;
}) {
  const { t } = useI18n();
  const NAV_ITEMS: { key: PageKey; label: string; icon: (cls: string) => ReactNode }[] = [
    { key: "sync", label: t.navSync, icon: (cls) => <IconBranch className={cls} /> },
    { key: "providers", label: t.navProviders, icon: (cls) => <IconCloud className={cls} /> },
    { key: "logs", label: t.navLogs, icon: (cls) => <IconList className={cls} /> },
    { key: "mcp", label: t.navMcp, icon: (cls) => <IconPlug className={cls} /> },
    { key: "settings", label: t.navSettings, icon: (cls) => <IconSettings className={cls} /> },
  ];
  return (
    <div className="flex h-full">
      <aside className="flex w-52 shrink-0 flex-col border-r bg-muted/30">
        <div className="flex items-center gap-2.5 px-4 py-4">
          <Logo className="h-8 w-8" />
          <div className="leading-tight">
            <div className="text-sm font-semibold">Git Repo Sync</div>
            <div className="text-xs text-muted-foreground">v{version}</div>
          </div>
        </div>
        <nav className="flex-1 space-y-1 px-2">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.key}
              className={cn(
                "flex w-full items-center gap-2.5 rounded-md px-3 py-2 text-sm font-medium transition-colors",
                page === item.key
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-accent hover:text-foreground",
              )}
              onClick={() => onNavigate(item.key)}
            >
              {item.icon("h-4 w-4")}
              {item.label}
            </button>
          ))}
        </nav>
        <div className="space-y-2 border-t px-3 py-3">
          <LangButton />
          <div className="px-1 text-xs text-muted-foreground">{t.currentUser(username)}</div>
          <Button variant="ghost" size="sm" className="w-full justify-start" onClick={onLogout}>
            <IconLogout className="h-4 w-4" />
            {t.signOut}
          </Button>
        </div>
      </aside>
      <main className="min-w-0 flex-1 overflow-y-auto">{children}</main>
    </div>
  );
}
