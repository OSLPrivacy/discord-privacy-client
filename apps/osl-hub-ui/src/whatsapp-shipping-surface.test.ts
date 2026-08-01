import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { describe, expect, it } from "vitest";
import viteConfig from "../vite.config";

const features = (): string[] => {
  const cargo = readFileSync(new URL("../../osl-hub/Cargo.toml", import.meta.url), "utf8");
  const defaultFeatures = cargo.match(/^default\s*=\s*\[([^\]]*)\]/m);
  if (!defaultFeatures) throw new Error("Cargo default feature set is missing");
  return [...defaultFeatures[1].matchAll(/"([^"]+)"/g)].map((match) => match[1]);
};

describe("WhatsApp shipping surface", () => {
  it("is compiled by the standard desktop build and emitted into frontendDist", () => {
    expect(features()).toContain("whatsapp-qa-shell");

    const config = typeof viteConfig === "function"
      ? viteConfig({ command: "build", mode: "production", isSsrBuild: false, isPreview: false })
      : viteConfig;
    const input = config.build?.rollupOptions?.input;
    expect(input).toMatchObject({
      main: expect.any(String),
      whatsappQa: expect.any(String),
    });
    expect(basename((input as Record<string, string>).main)).toBe("index.html");
    expect(basename((input as Record<string, string>).whatsappQa)).toBe("whatsapp-qa.html");

    const tauri = JSON.parse(readFileSync(new URL("../../osl-hub/tauri.conf.json", import.meta.url), "utf8"));
    expect(tauri.build.frontendDist).toBe("../osl-hub-ui/dist");
  });
});
