import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import {defineConfig} from "vite";

// `npm run dev` proxies the API to `pulply serve` (default 127.0.0.1:8484). Open the dev
// server with `#token=<token printed by pulply serve --token ...>`.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/api": {target: process.env.PULPLY_URL ?? "http://127.0.0.1:8484", changeOrigin: true},
    },
  },
  build: {chunkSizeWarningLimit: 2000},
});
