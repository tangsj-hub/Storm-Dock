import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { resolve } from "node:path";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 1420, strictPort: true },
  build: {
    rollupOptions: {
      input: {
        accounts: resolve(__dirname, "index.html"),
        add: resolve(__dirname, "add.html"),
        settings: resolve(__dirname, "settings.html"),
        usage: resolve(__dirname, "usage.html")
      }
    }
  },
  test: {
    include: ["src/pages/home/**/*.test.ts"],
  },
});
