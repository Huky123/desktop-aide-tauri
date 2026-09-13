import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  test: {
    environment: "happy-dom",
    setupFiles: ["./src/test/setup.ts"],
    globals: true,
    css: true,
    include: ["src/**/*.test.{ts,tsx}"],
    exclude: ["node_modules", "src-tauri"],
  },
  // 排除 @tauri-apps/api 的预构建（与 vite.config.ts 保持一致）
  optimizeDeps: {
    exclude: ["@tauri-apps/api"],
  },
});
