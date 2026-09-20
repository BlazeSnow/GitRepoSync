import { useEffect } from "react";
import { cn } from "@/lib/utils";

export interface ContextMenuItem {
  label: string;
  danger?: boolean;
  onSelect: () => void;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  useEffect(() => {
    const close = () => onClose();
    // 菜单外按下鼠标即关闭（mousedown 比 click 更跟手）；resize 换位置后关闭
    window.addEventListener("mousedown", close);
    window.addEventListener("click", close);
    window.addEventListener("contextmenu", close);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("click", close);
      window.removeEventListener("contextmenu", close);
      window.removeEventListener("resize", close);
    };
  }, [onClose]);

  const menuW = 160;
  const menuH = items.length * 36 + 8;
  const left = Math.min(x, window.innerWidth - menuW - 8);
  const top = Math.min(y, window.innerHeight - menuH - 8);

  return (
    <div
      className="fixed z-50 min-w-40 rounded-md border bg-popover p-1 text-popover-foreground shadow-md"
      style={{ left, top }}
      onClick={(e) => e.stopPropagation()}
      // 菜单内的按下/右键不冒泡到 window 的关闭监听，避免菜单瞬间自关
      onMouseDown={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
    >
      {items.map((item) => (
        <button
          key={item.label}
          className={cn(
            "block w-full rounded-sm px-2 py-1.5 text-left text-sm outline-none hover:bg-accent",
            item.danger && "text-destructive hover:bg-destructive/10",
          )}
          onClick={() => {
            onClose();
            item.onSelect();
          }}
        >
          {item.label}
        </button>
      ))}
    </div>
  );
}
