import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { attachmentProgressMarkup, parseAttachmentProgressEvent } from "./attachment-progress";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";
import { shippedHubCspHeaders } from "../../../scripts/lib/csp-mirror.mjs";

const servers: ReturnType<typeof createServer>[] = [];
const stylesheet = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => new Promise<void>((resolve, reject) => {
    server.closeAllConnections();
    server.close((error) => error ? reject(error) : resolve());
  })));
});

describe("TF-61 attachment progress UI", () => {
  it("labels every broker stage without inventing a completion state", () => {
    for (const [stage, label] of [
      ["selected", "Ready to protect attachment"],
      ["protecting", "Protecting attachment"],
      ["uploading", "Uploading protected attachment"],
      ["delivering", "Delivering attachment"],
      ["sent", "Attachment sent"],
      ["failed", "Attachment failed"],
      ["cancelled", "Attachment cancelled"],
    ] as const) {
      const markup = attachmentProgressMarkup({
        contextId: "chat:opaque-42",
        job: {
          jobId: "AbCdEfGhIjKlMnOpQrStUv",
          metadata: { filename: "photo.png", mediaType: "image/png", size: 512 },
          caption: "",
          viewOnce: false,
          stage,
          progress: stage === "sent" ? 100 : 0,
          retryFrom: null,
          failure: null,
        },
      });
      expect(markup).toContain(`>${label}<`);
      expect(markup).toContain(`data-attachment-stage="${stage}"`);
    }
  });

  it("renders each active stage as honest, externally-styled progress under the shipped CSP", async () => {
    const event = parseAttachmentProgressEvent({
      contextId: "chat:opaque-42",
      job: {
        jobId: "AbCdEfGhIjKlMnOpQrStUv",
        metadata: { filename: "photo.png", mediaType: "image/png", size: 512 },
        caption: "",
        viewOnce: false,
        stage: "uploading",
        progress: 50,
        retryFrom: null,
        failure: null,
      },
    });
    expect(event).not.toBeNull();
    if (!event) throw new Error("valid attachment event was refused");

    const server = createServer((request, response) => {
      if (request.url === "/styles.css") {
        response.writeHead(200, { "content-type": "text/css; charset=utf-8" });
        response.end(stylesheet);
        return;
      }
      response.writeHead(200, {
        ...shippedHubCspHeaders(),
        "content-type": "text/html; charset=utf-8",
      });
      response.end(`<!doctype html><link rel="stylesheet" href="/styles.css"><main>${attachmentProgressMarkup(event)}</main>`);
    });
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("could not start attachment progress fixture server");

    const chrome = await launchChrome();
    try {
      const page = await chrome.openPage();
      try {
        await page.navigate(`http://127.0.0.1:${address.port}/`);
        const rendered = await page.evaluate(`(() => {
          const surface = document.querySelector('.attachment-progress');
          const progress = document.querySelector('.attachment-progress__bar');
          const stage = document.querySelector('.attachment-progress__stage');
          if (!(surface instanceof HTMLElement) || !(progress instanceof HTMLProgressElement) || !(stage instanceof HTMLElement)) throw new Error('missing attachment progress surface');
          return {
            value: progress.value,
            max: progress.max,
            width: progress.getBoundingClientRect().width,
            surfaceStyle: surface.getAttribute('style'),
            progressStyle: progress.getAttribute('style'),
            stage: stage.textContent,
          };
        })()`);

        expect(rendered).toMatchObject({
          value: 50,
          max: 100,
          stage: "Uploading protected attachment",
          surfaceStyle: null,
          progressStyle: null,
        });
        expect(rendered.width).toBeGreaterThan(0);
      } finally {
        await page.close();
      }
    } finally {
      await chrome.close();
    }
  });
});
