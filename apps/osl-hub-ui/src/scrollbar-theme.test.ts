import { execFile } from "node:child_process";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { createServer } from "node:http";
import { homedir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

interface ScrollbarColours {
  thumb: string;
  track: string;
}

function locateChrome(): string {
  const root = join(homedir(), ".cache", "ms-playwright");
  for (const directory of readdirSync(root).sort().reverse()) {
    for (const relative of ["chrome-linux64/chrome", "chrome-headless-shell-linux64/chrome-headless-shell", "chrome-linux/headless_shell"]) {
      const candidate = join(root, directory, relative);
      if (existsSync(candidate)) return candidate;
    }
  }
  throw new Error("TU-33 requires the Playwright Chromium binary");
}

async function computedScrollbarColours(theme: "dark" | "light"): Promise<ScrollbarColours> {
  const css = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  const themeAttribute = theme === "light" ? ' data-theme="light"' : "";
  const document = `<!doctype html><html${themeAttribute}><head><link rel="stylesheet" href="/styles.css"></head><body><script>
    document.body.textContent = JSON.stringify({
      thumb: getComputedStyle(document.documentElement, "::-webkit-scrollbar-thumb").backgroundColor,
      track: getComputedStyle(document.documentElement, "::-webkit-scrollbar-track").backgroundColor,
    });
  </script></body></html>`;
  const server = createServer((request, response) => {
    if (request.url === "/styles.css") {
      response.writeHead(200, { "content-type": "text/css" });
      response.end(css);
      return;
    }
    response.writeHead(200, { "content-type": "text/html" });
    response.end(document);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("Could not start TU-33 stylesheet server");
  try {
    const output = await new Promise<string>((resolve, reject) => execFile(locateChrome(), [
      "--headless=new",
      "--no-sandbox",
      "--disable-gpu",
      "--dump-dom",
      `http://127.0.0.1:${address.port}/`,
    ], { encoding: "utf8", timeout: 20_000 }, (error, stdout) => error ? reject(error) : resolve(stdout)));
    const json = output.match(/<body[^>]*>(.*?)<\/body>/su)?.[1];
    if (!json) throw new Error("Chromium did not return computed scrollbar colours");
    return JSON.parse(json) as ScrollbarColours;
  } finally {
    await new Promise<void>((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
}

describe("TU-33 scrollbar theme", () => {
  it.each([
    ["dark", "rgb(77, 89, 104)", "rgb(20, 20, 20)"],
    ["light", "rgb(115, 130, 148)", "rgb(220, 227, 233)"],
  ] as const)("computes the %s thumb and track colours", async (theme, thumb, track) => {
    await expect(computedScrollbarColours(theme)).resolves.toEqual({
      thumb,
      track,
    });
  });
});
