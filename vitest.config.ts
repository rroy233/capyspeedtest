import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Vitest 使用 jsdom 模拟浏览器环境，便于测试 React 路由与组件。
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: "./src/setupTests.ts",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
  },
});
