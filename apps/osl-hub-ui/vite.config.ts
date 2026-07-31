import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

const discordQaRendererDefine = (mode: string) => mode === "discord-qa"
  ? { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("1") }
  : { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("0") };

export default defineConfig(({ mode }) => ({
  base: "./",
  define: {
    ...discordQaRendererDefine(mode),
    "import.meta.env.VITE_OSL_SIGNAL_QA_SHELL": JSON.stringify(mode === "signal-qa" ? "1" : "0"),
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    modulePreload: false,
    rollupOptions: {
      input: {
        // Naming any input disables Vite's implicit index.html entry, so the
        // main window's page silently stopped being built when the QA pages
        // were added. It must stay listed explicitly.
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        ...(mode === "signal-qa" ? {
          "/src/main.ts": fileURLToPath(new URL("./src/signal-qa-main.ts", import.meta.url)),
        } : {}),
        whatsappQa: fileURLToPath(new URL("./whatsapp-qa.html", import.meta.url)),
        whatsappOverlay: fileURLToPath(new URL("./whatsapp-overlay.html", import.meta.url)),
        overlay: fileURLToPath(new URL("./overlay.html", import.meta.url)),
        shield: fileURLToPath(new URL("./shield.html", import.meta.url)),
      },
    },
  },
}));
