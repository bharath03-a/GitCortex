import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    proxy: {
      "/data": "http://localhost:5678",
      "/api": "http://localhost:5678",
    },
  },
  build: {
    outDir: "../crates/gitcortex-viz/dist-viz",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: "assets/main.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: (info) => {
          if (info.name?.endsWith(".css")) return "assets/main.css";
          return "assets/[name][extname]";
        },
      },
    },
  },
  worker: {
    // Vite builds Worker entry points as a separate bundle; without this the
    // chunk gets a content hash in its name, which the Axum server (fixed,
    // hardcoded /assets/* routes, no wildcard static serving) can't route to.
    rollupOptions: {
      output: {
        entryFileNames: "assets/[name].js",
      },
    },
  },
});
