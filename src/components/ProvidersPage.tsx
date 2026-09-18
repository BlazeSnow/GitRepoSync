import { useEffect, useState } from "react";
import { api } from "@/lib/api";
import type { AccountInfo, ProviderInfo, ProviderPlatform } from "@/lib/types";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useTranslation } from "react-i18next";

const PLATFORMS: { key: ProviderPlatform; title: string }[] = [
  { key: "github", title: "GitHub" },
  { key: "gitlab", title: "GitLab" },
];

export function ProvidersPage({ token }: { token: string }) {
  const { t } = useTranslation();
  const [providers, setProviders] = useState<ProviderInfo[]>([]);
  const [pats, setPats] = useState<Record<string, string>>({ github: "", gitlab: "" });
  const [accounts, setAccounts] = useState<Record<string, AccountInfo | null>>({});
  const [loading, setLoading] = useState<Record<string, boolean>>({});
  const [messages, setMessages] = useState<Record<string, { text: string; ok: boolean }>>({});

  useEffect(() => {
    api
      .getProviders(token)
      .then(setProviders)
      .catch((e) => setMessages({ github: { text: String(e), ok: false } }));
  }, [token]);

  function infoOf(platform: string): ProviderInfo | undefined {
    return providers.find((p) => p.platform === platform);
  }

  async function handleSaveAndFetch(platform: ProviderPlatform) {
    setMessages((m) => ({ ...m, [platform]: { text: "", ok: true } }));
    setLoading((l) => ({ ...l, [platform]: true }));
    try {
      const pat = (pats[platform] ?? "").trim();
      if (pat) {
        await api.saveProvider(token, platform, pat);
        setPats((p) => ({ ...p, [platform]: "" }));
      }
      const account = await api.fetchAccounts(token, platform);
      setAccounts((a) => ({ ...a, [platform]: account }));
      const fresh = await api.getProviders(token);
      setProviders(fresh);
      setMessages((m) => ({ ...m, [platform]: { text: t("fetchOk"), ok: true } }));
    } catch (err) {
      setMessages((m) => ({ ...m, [platform]: { text: String(err), ok: false } }));
    } finally {
      setLoading((l) => ({ ...l, [platform]: false }));
    }
  }

  return (
    <div className="p-6">
      <h1 className="mb-1 text-lg font-semibold">{t("providersTitle")}</h1>
      <p className="mb-4 text-sm text-muted-foreground">{t("providersDesc")}</p>
      <div className="grid gap-4 lg:grid-cols-2">
        {PLATFORMS.map(({ key, title }) => {
          const info = infoOf(key);
          const account = accounts[key];
          return (
            <Card key={key}>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  {title}
                  {info?.hasPat && (
                    <span className="rounded bg-secondary px-1.5 py-0.5 text-xs text-secondary-foreground">
                      {t("patConfigured", { mask: info.patMasked ?? "" })}
                    </span>
                  )}
                </CardTitle>
                <CardDescription>
                  {key === "github" ? t("githubHint") : t("gitlabHint")}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-3">
                <Input
                  type="password"
                  placeholder={info?.hasPat ? t("patPlaceholderSaved") : t("patPlaceholderNew")}
                  value={pats[key] ?? ""}
                  onChange={(e) => setPats((p) => ({ ...p, [key]: e.target.value }))}
                />
                <div className="flex items-center gap-3">
                  <Button
                    size="sm"
                    onClick={() => handleSaveAndFetch(key)}
                    disabled={loading[key]}
                  >
                    {loading[key] ? t("fetching") : t("saveAndFetch")}
                  </Button>
                  {messages[key]?.text && (
                    <span
                      className={`text-xs ${messages[key].ok ? "text-success" : "text-destructive"}`}
                    >
                      {messages[key].text}
                    </span>
                  )}
                </div>

                {account && (
                  <div className="space-y-3 rounded-lg border p-3">
                    <div className="flex items-center gap-3">
                      {account.avatarUrl ? (
                        <img src={account.avatarUrl} alt="" className="h-10 w-10 rounded-full" />
                      ) : (
                        <div className="flex h-10 w-10 items-center justify-center rounded-full bg-secondary text-sm">
                          {account.login.slice(0, 1).toUpperCase()}
                        </div>
                      )}
                      <div className="leading-tight">
                        <div className="text-sm font-medium">{account.name}</div>
                        <div className="text-xs text-muted-foreground">{account.login}</div>
                      </div>
                    </div>
                    <div>
                      <div className="mb-1.5 text-xs font-medium text-muted-foreground">
                        {t("orgs", { count: account.orgs.length })}
                      </div>
                      {account.orgs.length === 0 ? (
                        <div className="text-xs text-muted-foreground">{t("noOrgs")}</div>
                      ) : (
                        <div className="flex flex-wrap gap-1.5">
                          {account.orgs.map((org) => (
                            <span
                              key={org.login}
                              title={org.description}
                              className="inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs"
                            >
                              {org.avatarUrl && (
                                <img src={org.avatarUrl} alt="" className="h-4 w-4 rounded" />
                              )}
                              {org.name}
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </CardContent>
            </Card>
          );
        })}
      </div>
    </div>
  );
}
