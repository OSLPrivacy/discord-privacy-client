import { createServer } from "node:http";
import { afterEach, describe, expect, it } from "vitest";

import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { shippedHubCspHeaders } from "../../../scripts/lib/csp-mirror.mjs";

const servers: ReturnType<typeof createServer>[] = [];

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => new Promise<void>((resolve, reject) => {
    server.closeAllConnections();
    server.close((error) => error ? reject(error) : resolve());
  })));
});

describe("TU-07 shipped CSP render gate", () => {
  it("drops a fixture whose only styling is an inline style attribute", async () => {
    const server = createServer((_request, response) => {
      response.writeHead(200, {
        ...shippedHubCspHeaders(),
        "content-type": "text/html; charset=utf-8",
      });
      response.end('<!doctype html><main><aside id="inline-only" style="width: 232px">fixture</aside></main>');
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
        const width = await page.evaluate("getComputedStyle(document.querySelector('#inline-only')).width");
        expect(width).not.toBe("232px");
      } finally {
        await page.close();
      }
    } finally {
      await chrome.close();
    }
  });
});
