import { defineConfig } from "vite";
import { fileURLToPath } from "node:url";

export default defineConfig({
  base: "./",
  build: {
    outDir: "dist",
    emptyOutDir: true,
    modulePreload: false,
    rollupOptions: {
      input: {
        whatsappQa: fileURLToPath(new URL("./whatsapp-qa.html", import.meta.url)),
      },
    },
  },
});
