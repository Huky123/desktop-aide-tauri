import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],

  // 防止 Vite 混淆 Tauri 在 Linux/macOS 上的 Rust 文件
  clearScreen: false,

  server: {
    // Tauri 期望在固定端口上运行
    port: 5173,
    strictPort: true,
    // 允许 Tauri 从开发服务器加载资源
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },

  // 排除 @tauri-apps/api 及其插件的预构建 —— Tauri API 依赖运行时的 window.__TAURI_INTERNALS__
  // 预构建可能导致 IPC 桥接在运行时不可用
  optimizeDeps: {
    exclude: ["@tauri-apps/api", "@tauri-apps/plugin-dialog"],
  },
});
