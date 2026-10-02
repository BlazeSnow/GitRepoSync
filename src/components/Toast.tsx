// Toast 通知：操作结果与同步事件的实时提示（比日志页更及时、附完整失败原因）。
// 手写实现（项目无 toast 依赖）：右下角堆叠、motion 进出场动画、自动消失 + 手动关闭。
import {
  createContext,
  useCallback,
  useContext,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { IconCircleAlert, IconCircleCheck, IconInfo, IconX } from "@/components/icons";
import { cn } from "@/lib/utils";

export type ToastKind = "success" | "error" | "info";

export interface ToastOptions {
  kind: ToastKind;
  title: string;
  /** 可选详情（如同步失败的按目标完整原因，支持换行） */
  description?: string;
}

type ToastItem = ToastOptions & { id: number };

const ToastContext = createContext<(o: ToastOptions) => void>(() => {});

/** 弹出一条 toast；ToastProvider 外调用为安全空操作 */
export function useToast() {
  return useContext(ToastContext);
}

/** 同屏上限：超出挤掉最旧的，避免批量同步刷屏 */
const MAX_VISIBLE = 5;
const DISMISS_MS = 5000;
const ERROR_DISMISS_MS = 8000;

export function ToastProvider({
  children,
  dismissMs = DISMISS_MS,
  errorDismissMs = ERROR_DISMISS_MS,
}: {
  children: ReactNode;
  /** 自动消失时长（测试可注入更短值） */
  dismissMs?: number;
  errorDismissMs?: number;
}) {
  const { t } = useTranslation();
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const idRef = useRef(0);

  const dismiss = useCallback((id: number) => {
    setToasts((ts) => ts.filter((x) => x.id !== id));
  }, []);

  const push = useCallback(
    (o: ToastOptions) => {
      const id = ++idRef.current;
      setToasts((ts) => [...ts.slice(-(MAX_VISIBLE - 1)), { ...o, id }]);
      window.setTimeout(() => dismiss(id), o.kind === "error" ? errorDismissMs : dismissMs);
    },
    [dismiss, dismissMs, errorDismissMs],
  );

  const kindIcon: Record<ToastKind, (cls: string) => ReactNode> = {
    success: (cls) => <IconCircleCheck className={cn(cls, "text-success")} />,
    error: (cls) => <IconCircleAlert className={cn(cls, "text-destructive")} />,
    info: (cls) => <IconInfo className={cn(cls, "text-primary")} />,
  };

  return (
    <ToastContext.Provider value={push}>
      {children}
      {/* aria-live：屏幕阅读器播报通知 */}
      <div
        aria-live="polite"
        className="pointer-events-none fixed bottom-4 right-4 z-[60] flex w-96 max-w-[calc(100vw-2rem)] flex-col gap-2"
      >
        <AnimatePresence initial={false}>
          {toasts.map((x) => (
            <motion.div
              key={x.id}
              layout
              initial={{ opacity: 0, y: 12, scale: 0.97 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={{ opacity: 0, x: 24 }}
              transition={{ duration: 0.18, ease: "easeOut" }}
              className="pointer-events-auto flex items-start gap-2.5 rounded-lg border bg-popover p-3 text-popover-foreground shadow-md"
            >
              <span className="mt-0.5 inline-flex h-4 w-4 shrink-0 items-center justify-center">
                {kindIcon[x.kind]("h-4 w-4")}
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-sm font-medium leading-snug">{x.title}</p>
                {x.description && (
                  <p className="mt-1 whitespace-pre-wrap break-words text-xs leading-relaxed text-muted-foreground">
                    {x.description}
                  </p>
                )}
              </div>
              <button
                type="button"
                aria-label={t("close")}
                title={t("close")}
                className="rounded-sm p-0.5 text-muted-foreground hover:bg-accent hover:text-foreground"
                onClick={() => dismiss(x.id)}
              >
                <IconX className="h-3.5 w-3.5" />
              </button>
            </motion.div>
          ))}
        </AnimatePresence>
      </div>
    </ToastContext.Provider>
  );
}
