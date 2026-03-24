import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
    clearMocks: true,
    setupFiles: ["./src/__tests__/tauri.setup.ts"],
  },
});
