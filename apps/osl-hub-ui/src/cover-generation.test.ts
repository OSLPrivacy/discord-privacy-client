import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { shippedHubCspHeaders } from "../../../scripts/lib/csp-mirror.mjs";

const servers: ReturnType<typeof createServer>[] = [];
const stylesheet = readFileSync(new URL("./cover-generation.css", import.meta.url), "utf8");

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => new Promise<void>((resolve, reject) => {
    server.closeAllConnections();
    server.close((error) => error ? reject(error) : resolve());
  })));
});

describe("TU-80 cover-generation progress presentation", () => {
  it("renders and animates the progress bar from a self-hosted stylesheet under the shipped CSP", async () => {
    const server = createServer((request, response) => {
      if (request.url === "/cover-generation.css") {
        response.writeHead(200, { "content-type": "text/css; charset=utf-8" });
        response.end(stylesheet);
        return;
      }

      response.writeHead(200, {
        ...shippedHubCspHeaders(),
        "content-type": "text/html; charset=utf-8",
      });
      response.end(`<!doctype html>
        <link rel="stylesheet" href="/cover-generation.css">
        <main>
          <section class="cover-generation-surface" role="status" aria-live="polite">
            <div class="cover-generation-progress" aria-hidden="true"><span class="cover-generation-progress-fill"></span></div>
            <div class="cover-generation-popup">Cover text generating</div>
          </section>
        </main>`);
    });
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("could not start CSP fixture server");

    const chrome = await launchChrome();
    try {
      const page = await chrome.openPage();
      try {
        await page.navigate(`http://127.0.0.1:${address.port}/`);
        const rendered = await page.evaluate(`(() => {
          const fill = document.querySelector('.cover-generation-progress-fill');
          const popup = document.querySelector('.cover-generation-popup');
          if (!(fill instanceof HTMLElement) || !(popup instanceof HTMLElement)) throw new Error('missing progress fixture');
          const fillStyle = getComputedStyle(fill);
          const popupStyle = getComputedStyle(popup);
          return {
            fillWidth: fill.getBoundingClientRect().width,
            animationName: fillStyle.animationName,
            popupPosition: popupStyle.position,
          };
        })()`);

        expect(rendered.fillWidth).toBeGreaterThan(0);
        expect(rendered.animationName).not.toBe("none");
        expect(rendered.popupPosition).toBe("fixed");
      } finally {
        await page.close();
      }
    } finally {
      await chrome.close();
    }
  });
});
