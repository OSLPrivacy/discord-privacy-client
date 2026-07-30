import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

export default defineConfig(({ mode }) => ({
  base: "./",
  define: mode === "discord-qa"
    ? { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("1") }
    : {},
  build: {
    outDir: "dist",
    emptyOutDir: true,
    modulePreload: false,
    rollupOptions: {
      input: {
        whatsappQa: fileURLToPath(new URL("./whatsapp-qa.html", import.meta.url)),
        whatsappOverlay: fileURLToPath(new URL("./whatsapp-overlay.html", import.meta.url)),
        overlay: fileURLToPath(new URL("./overlay.html", import.meta.url)),
        shield: fileURLToPath(new URL("./shield.html", import.meta.url)),
      },
    },
  },
}));
