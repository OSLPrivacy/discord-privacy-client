import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

export default defineConfig(() => {
  const signalQaShell = process.env.VITE_OSL_SIGNAL_QA_SHELL === "1";
  return {
    base: "./",
    resolve: signalQaShell ? {
      alias: {
        "/src/main.ts": fileURLToPath(new URL("./src/signal-qa-main.ts", import.meta.url)),
      },
    } : undefined,
    build: {
      outDir: "dist",
      emptyOutDir: true,
      modulePreload: false,
      rollupOptions: {
        input: {
          main: fileURLToPath(new URL("./index.html", import.meta.url)),
          overlay: fileURLToPath(new URL("./overlay.html", import.meta.url)),
        },
      },
    },
  };
});
