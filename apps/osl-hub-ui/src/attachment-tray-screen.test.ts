import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it } from "vitest";

import {
  attachmentTrayCardMarkup,
  attachmentTrayScreenMarkup,
  attachmentTraySizeLabel,
  isAttachmentTrayPicture,
  type AttachmentTrayCard,
} from "./attachment-tray-screen";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ARTIFACT_DIR = path.join(HERE, "..", "screenshots", "artifacts");
const PNG_PATH = path.join(ARTIFACT_DIR, "task-0625-attachment-tray.png");

const stylesheet = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

/** 1x1 PNG so the picture card has something real to render as a preview. */
const ONE_PIXEL_PNG = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

const PICTURE_CARD: AttachmentTrayCard = {
  removableId: "removable-picture-1",
  name: "vacation.png",
  type: "image/png",
  size: 245_760,
  previewDataUrl: ONE_PIXEL_PNG,
};

const DOCUMENT_CARD: AttachmentTrayCard = {
  removableId: "removable-document-1",
  name: "Quarterly Report.pdf",
  type: "application/pdf",
  size: 4_096,
  previewDataUrl: null,
};

const servers: ReturnType<typeof createServer>[] = [];

afterEach(async () => {
  await Promise.all(servers.splice(0).map((server) => new Promise<void>((resolve, reject) => {
    server.closeAllConnections();
    server.close((error) => error ? reject(error) : resolve());
  })));
});

function pngDimensions(buffer: Buffer): { width: number; height: number } {
  expect(buffer.subarray(0, 8).toString("hex")).toBe("89504e470d0a1a0a");
  return { width: buffer.readUInt32BE(16), height: buffer.readUInt32BE(20) };
}

describe("TASK 0625 attachment tray screen", () => {
  it("classifies picture vs document types", () => {
    expect(isAttachmentTrayPicture("image/png")).toBe(true);
    expect(isAttachmentTrayPicture("image/jpeg")).toBe(true);
    expect(isAttachmentTrayPicture("application/pdf")).toBe(false);
    expect(isAttachmentTrayPicture("text/plain")).toBe(false);
  });

  it("formats sizes for readable card metadata", () => {
    expect(attachmentTraySizeLabel(512)).toBe("512 B");
    expect(attachmentTraySizeLabel(4_096)).toBe("4.0 KB");
    expect(attachmentTraySizeLabel(245_760)).toBe("240 KB");
  });

  it("renders one card per file with name, type, size, and a remove button", () => {
    const markup = attachmentTrayCardMarkup(DOCUMENT_CARD);
    expect(markup).toContain('data-attachment-tray-card="removable-document-1"');
    expect(markup).toContain(">Quarterly Report.pdf<");
    expect(markup).toContain("application/pdf");
    expect(markup).toContain("4.0 KB");
    expect(markup).toContain('data-attachment-tray-remove="removable-document-1"');
  });

  it("only renders an image preview for picture cards", () => {
    const pictureMarkup = attachmentTrayCardMarkup(PICTURE_CARD);
    expect(pictureMarkup).toContain(`<img class="attachment-tray-card__preview" src="${ONE_PIXEL_PNG}"`);

    const documentMarkup = attachmentTrayCardMarkup(DOCUMENT_CARD);
    expect(documentMarkup).not.toContain("<img");
    expect(documentMarkup).toContain("attachment-tray-card__preview--none");
  });

  it("shows an empty state when the tray has no files", () => {
    expect(attachmentTrayScreenMarkup([])).toContain("No files in the tray.");
  });

  it("captures a Linux screenshot of one picture and one document as two cards with one preview", async () => {
    mkdirSync(ARTIFACT_DIR, { recursive: true });
    const markup = attachmentTrayScreenMarkup([PICTURE_CARD, DOCUMENT_CARD]);

    const server = createServer((request, response) => {
      if (request.url === "/styles.css") {
        response.writeHead(200, { "content-type": "text/css; charset=utf-8" });
        response.end(stylesheet);
        return;
      }
      // Not using shippedHubCspHeaders(): apps/osl-hub/tauri.conf.json currently has a
      // pre-existing duplicate-key JSON syntax error from an unrelated lane merge.
      response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
      response.end(`<!doctype html><meta name="color-scheme" content="dark"><link rel="stylesheet" href="/styles.css"><body style="background:#0a0a0a;padding:24px"><main style="width:420px">${markup}</main></body>`);
    });
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error("could not start attachment tray fixture server");

    const chrome = await launchChrome();
    try {
      const page = await chrome.openPage();
      try {
        await page.navigate(`http://127.0.0.1:${address.port}/`);
        const rendered = await page.evaluate(`(() => {
          const cards = [...document.querySelectorAll('.attachment-tray-card')];
          return {
            cardCount: cards.length,
            kinds: cards.map((card) => card.dataset.attachmentTrayKind),
            previewImages: document.querySelectorAll('.attachment-tray-card__preview[src]').length,
            removeButtons: document.querySelectorAll('.attachment-tray-card__remove').length,
            names: cards.map((card) => card.querySelector('.attachment-tray-card__name')?.textContent),
          };
        })()`);

        expect(rendered).toMatchObject({
          cardCount: 2,
          kinds: ["picture", "document"],
          previewImages: 1,
          removeButtons: 2,
          names: ["vacation.png", "Quarterly Report.pdf"],
        });

        const png = await page.screenshot({ fromSurface: true });
        writeFileSync(PNG_PATH, png);
        const dimensions = pngDimensions(png);
        expect(dimensions.width).toBeGreaterThan(0);
        expect(dimensions.height).toBeGreaterThan(0);
        expect(png.length).toBeGreaterThan(5_000);
        const uniqueBytes = new Set(png).size;
        expect(uniqueBytes).toBeGreaterThan(16);

        console.log(`TASK0625_PNG=${PNG_PATH}`);
        console.log(`TASK0625_PNG_DIMENSIONS=${dimensions.width}x${dimensions.height}`);
        console.log(`TASK0625_PNG_BYTES=${png.length}`);
        console.log(`TASK0625_CARD_COUNT=${rendered.cardCount}`);
        console.log(`TASK0625_KINDS=${rendered.kinds.join(",")}`);
        console.log(`TASK0625_PREVIEW_IMAGES=${rendered.previewImages}`);
        console.log(`TASK0625_REMOVE_BUTTONS=${rendered.removeButtons}`);
      } finally {
        await page.close();
      }
    } finally {
      await chrome.close();
    }
  }, 30_000);
});
