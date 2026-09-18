import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { api } from "@/lib/api";
import { LangButton } from "@/components/LangButton";
import { Logo } from "@/components/icons";

export function Login({
  onLogin,
}: {
  onLogin: (token: string, username: string) => void;
}) {
  const { t } = useTranslation();
  const [username, setUsername] = useState("admin");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError("");
    setLoading(true);
    try {
      const result = await api.login(username.trim(), password, remember);
      onLogin(result.token, result.username);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="flex h-full items-center justify-center bg-muted/40">
      <Card className="w-[380px]">
        <CardHeader className="items-center text-center">
          <div className="mx-auto flex w-full items-center justify-between">
            <span />
            <Logo className="h-16 w-16" />
            <LangButton />
          </div>
          <CardTitle className="text-xl">Git Repo Sync</CardTitle>
          <CardDescription>{t("loginSubtitle")}</CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-1.5">
              <Label htmlFor="username">{t("username")}</Label>
              <Input
                id="username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                autoComplete="username"
                autoFocus
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="password">{t("password")}</Label>
              <Input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoComplete="current-password"
              />
            </div>
            <label className="flex items-center gap-2 text-sm text-muted-foreground">
              <input
                type="checkbox"
                className="h-4 w-4 accent-primary"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              {t("remember")}
            </label>
            {error && <p className="text-sm text-destructive">{error}</p>}
            <Button type="submit" className="w-full" disabled={loading}>
              {loading ? t("signingIn") : t("signIn")}
            </Button>
            <p className="text-center text-xs text-muted-foreground">{t("loginHint")}</p>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
