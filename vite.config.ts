import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dirname = path.dirname(fileURLToPath(import.meta.url));

// https://vitest.dev/config/（defineConfig 兼容 vite 原有字段，tauri CLI 照常读取）
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: {
      "@": path.resolve(dirname, "./src"),
      // 应用图标产物（由 pnpm tauri icon 从用户图标源图生成）
      "~icons": path.resolve(dirname, "./src-tauri/icons"),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: false,
    // 关键：不监听 src-tauri（cargo 编译会锁定 target 下的 exe，导致 fs.watch EBUSY 崩溃）
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    target: "chrome105",
    sourcemap: false,
    // 桌面应用：前端资源随安装包本地加载（无网络开销），不做代码拆分；
    // 单 chunk 超过 Web 默认的 500 kB 阈值属预期，放宽以消除构建告警
    chunkSizeWarningLimit: 800,
  },
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
  },
});
