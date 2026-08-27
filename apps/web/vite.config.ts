import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";
import { VitePWA } from "vite-plugin-pwa";

export default defineConfig({
  base: "./",
  plugins: [
    svelte(),
    VitePWA({
      registerType: "autoUpdate",
      includeAssets: ["icons/icon.svg"],
      manifest: {
        name: "camcomp",
        short_name: "camcomp",
        description: "Compare local photos from different cameras and modes",
        start_url: ".",
        scope: ".",
        display: "standalone",
        background_color: "#111820",
        theme_color: "#111820",
        categories: ["photo", "utilities"],
        icons: [
          {
            src: "icons/icon.svg",
            sizes: "any",
            type: "image/svg+xml",
            purpose: "any maskable",
          },
        ],
      },
      workbox: {
        cleanupOutdatedCaches: true,
        maximumFileSizeToCacheInBytes: 4 * 1024 * 1024,
        globPatterns: ["**/*.{css,html,js,svg,txt,webmanifest}"],
        navigateFallback: "index.html",
        runtimeCaching: [],
      },
    }),
  ],
  resolve: {
    conditions: ["browser"],
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
  },
  preview: {
    host: "127.0.0.1",
    port: 4173,
    strictPort: true,
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./tests/setup.ts"],
    include: ["tests/**/*.test.ts"],
    css: true,
  },
});
