import { useTranslation } from "react-i18next";
import { changeAppLang } from "@/i18n";

/** 语言切换按钮：显示另一种语言的名称 */
export function LangButton() {
  const { i18n } = useTranslation();
  const isZh = i18n.language.toLowerCase().startsWith("zh");
  return (
    <button
      type="button"
      className="rounded-md border px-2 py-1 text-xs text-muted-foreground hover:bg-accent hover:text-foreground"
      onClick={() => void changeAppLang(isZh ? "en" : "zh")}
    >
      {isZh ? "English" : "中文"}
    </button>
  );
}
