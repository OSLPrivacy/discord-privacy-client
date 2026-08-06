import assert from "node:assert/strict";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { inflateSync } from "node:zlib";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const ARTIFACT_DIR = join(ROOT, "screenshots", "evidence");
const PNG_PATH = join(ARTIFACT_DIR, "review-defaults-900x700.png");
const AX_PATH = join(ARTIFACT_DIR, "review-defaults-900x700.ax.json");
const WINDOW = Object.freeze({ width: 900, height: 700 });

async function reviewDefaultsFixture() {
  const outFile = join(tmpdir(), `osl-review-defaults-fixture-${process.pid}-${Date.now()}.mjs`);
  await esbuild.build({
    stdin: {
      contents: `
        import { firstRunOnboardingStepContract } from "${join(ROOT, "src", "state.ts")}";
        import { initialDeleteChoices, onboardingDeleteMarkup } from "${join(ROOT, "src", "onboarding-delete.ts")}";
        const title = firstRunOnboardingStepContract.find((step) => step.step === "review-defaults")?.title;
        if (title !== "Review defaults") throw new Error("review-defaults contract title changed");
        const markup = onboardingDeleteMarkup(initialDeleteChoices());
        if (!markup.includes('id="continue-defaults-review"')) throw new Error("review defaults markup lost Continue");
        export const routeTitle = title;
        export const bodyMarkup = markup.replace(
          '<div class="setup-footer onboarding-actions">',
          '<div class="setup-footer onboarding-actions"><button class="button ghost onboarding-back" id="onboarding-back" aria-label="Back" type="button">Back</button>'
        );
      `,
      resolveDir: ROOT,
      sourcefile: "review-defaults-fixture-entry.mjs",
      loader: "js",
    },
    bundle: true,
    platform: "node",
    format: "esm",
    outfile: outFile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  return import(`file://${outFile}`);
}

function fixtureHtml({ routeTitle, bodyMarkup }) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "onboarding-controls.css"), "utf8"),
    readFileSync(join(ROOT, "src", "onboarding-delete.css"), "utf8"),
    `
      html, body, #app { height: 100%; }
      .review-defaults-fixture-title {
        margin: 0;
        color: var(--osl-setup-second, #8b949e);
        font-family: "Onest Variable", Onest, var(--font-ui);
        font-size: 15px;
        font-weight: 600;
        text-align: center;
      }
      .onboarding-panel .del-title { margin-top: 22px; }
    `,
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <title>${routeTitle}</title>
        <style>${css}</style>
      </head>
      <body>
        <div id="app">
          <div class="app-frame with-titlebar">
            <div class="desktop-titlebar" aria-hidden="true"><div class="desktop-drag-region"></div></div>
            <div class="onboarding-shell">
              <main class="onboarding-panel onboarding-defaults" aria-label="${routeTitle}">
                <p class="review-defaults-fixture-title">${routeTitle}</p>
                ${bodyMarkup}
              </main>
            </div>
          </div>
        </div>
      </body>
    </html>`;
}

async function fixtureServer(html) {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return { server, url: `http://127.0.0.1:${port}/` };
}

function closeServer(server) {
  return new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
}

function readPng(buffer) {
  assert.equal(buffer.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
  let offset = 8;
  let width = 0;
  let height = 0;
  let colorType = 0;
  const data = [];
  while (offset < buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.subarray(offset + 4, offset + 8).toString("ascii");
    const chunk = buffer.subarray(offset + 8, offset + 8 + length);
    offset += 12 + length;
    if (type === "IHDR") {
      width = chunk.readUInt32BE(0);
      height = chunk.readUInt32BE(4);
      assert.equal(chunk[8], 8, "PNG must be 8-bit for this evidence decoder");
      colorType = chunk[9];
      assert.ok(colorType === 2 || colorType === 6, `unsupported PNG color type ${colorType}`);
    } else if (type === "IDAT") {
      data.push(chunk);
    } else if (type === "IEND") {
      break;
    }
  }
  const channels = colorType === 6 ? 4 : 3;
  const stride = width * channels;
  const inflated = inflateSync(Buffer.concat(data));
  const pixels = Buffer.alloc(width * height * 4);
  let source = 0;
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y += 1) {
    const filter = inflated[source];
    source += 1;
    const row = Buffer.from(inflated.subarray(source, source + stride));
    source += stride;
    for (let x = 0; x < stride; x += 1) {
      const left = x >= channels ? row[x - channels] : 0;
      const up = previous[x] ?? 0;
      const upLeft = x >= channels ? previous[x - channels] : 0;
      if (filter === 1) row[x] = (row[x] + left) & 0xff;
      else if (filter === 2) row[x] = (row[x] + up) & 0xff;
      else if (filter === 3) row[x] = (row[x] + Math.floor((left + up) / 2)) & 0xff;
      else if (filter === 4) {
        const p = left + up - upLeft;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - upLeft);
        row[x] = (row[x] + (pa <= pb && pa <= pc ? left : pb <= pc ? up : upLeft)) & 0xff;
      } else assert.equal(filter, 0, `unsupported PNG row filter ${filter}`);
    }
    for (let x = 0; x < width; x += 1) {
      const src = x * channels;
      const dst = (y * width + x) * 4;
      pixels[dst] = row[src];
      pixels[dst + 1] = row[src + 1];
      pixels[dst + 2] = row[src + 2];
      pixels[dst + 3] = channels === 4 ? row[src + 3] : 255;
    }
    previous = row;
  }
  return { width, height, pixels };
}

function pixelStats(image, rect = { x: 0, y: 0, width: image.width, height: image.height }) {
  const colors = new Map();
  const left = Math.max(0, Math.floor(rect.x));
  const top = Math.max(0, Math.floor(rect.y));
  const right = Math.min(image.width, Math.ceil(rect.x + rect.width));
  const bottom = Math.min(image.height, Math.ceil(rect.y + rect.height));
  let count = 0;
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const offset = (y * image.width + x) * 4;
      const key = `${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`;
      colors.set(key, (colors.get(key) ?? 0) + 1);
      count += 1;
    }
  }
  const dominant = Math.max(...colors.values());
  return { pixels: count, uniqueColors: colors.size, nonDominantPixels: count - dominant };
}

function axNames(nodes) {
  return nodes
    .map((node) => ({
      role: node.role?.value ?? "",
      name: node.name?.value ?? "",
    }))
    .filter((node) => node.name);
}

test("TASK 0370 captures Review defaults at a fixed window size", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const fixture = await reviewDefaultsFixture();
  const { server, url } = await fixtureServer(fixtureHtml(fixture));
  const chrome = await launchChrome();
  const page = await chrome.openPage();
  try {
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: WINDOW.height,
      deviceScaleFactor: 1,
      mobile: false,
    });
    await page.navigate(url, { timeoutMs: 10_000 });
    await page.send("Accessibility.enable");
    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axNames(axTree.nodes);
    writeFileSync(AX_PATH, JSON.stringify(names, null, 2));

    for (const exact of ["Review defaults", "Continue", "Back"]) {
      assert.ok(names.some((node) => node.name === exact), `screen tree missing ${exact}`);
    }

    const rects = await page.evaluate(`(() => {
      const rectFor = (selector) => {
        const element = document.querySelector(selector);
        if (!element) throw new Error("missing " + selector);
        const rect = element.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      return {
        title: rectFor(".review-defaults-fixture-title"),
        continue: rectFor("#continue-defaults-review"),
        back: rectFor("#onboarding-back"),
        visibleText: document.body.innerText,
      };
    })()`);
    assert.match(rects.visibleText, /Review defaults/u);
    assert.match(rects.visibleText, /Continue/u);
    assert.match(rects.visibleText, /Back/u);

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const image = readPng(png);
    assert.deepEqual({ width: image.width, height: image.height }, WINDOW);
    const whole = pixelStats(image);
    assert.ok(whole.uniqueColors >= 64, `PNG nearly blank: uniqueColors=${whole.uniqueColors}`);
    assert.ok(whole.nonDominantPixels >= 10_000, `PNG nearly blank: nonDominantPixels=${whole.nonDominantPixels}`);

    const titleStats = pixelStats(image, rects.title);
    const continueStats = pixelStats(image, rects.continue);
    const backStats = pixelStats(image, rects.back);
    assert.ok(titleStats.nonDominantPixels > 50, `Review defaults not visibly painted: ${JSON.stringify(titleStats)}`);
    assert.ok(continueStats.nonDominantPixels > 500, `Continue not visibly painted: ${JSON.stringify(continueStats)}`);
    assert.ok(backStats.nonDominantPixels > 20, `Back not visibly painted: ${JSON.stringify(backStats)}`);

    console.log(`TASK_0370_PNG ${PNG_PATH}`);
    console.log(`TASK_0370_AX ${AX_PATH}`);
    console.log(`TASK_0370_WINDOW ${image.width}x${image.height}`);
    console.log(`TASK_0370_SCREEN_TREE_NAMES ${names.map((node) => node.name).join(" | ")}`);
    console.log(`TASK_0370_IMAGE whole_unique_colors=${whole.uniqueColors} whole_non_dominant_pixels=${whole.nonDominantPixels} title_non_dominant_pixels=${titleStats.nonDominantPixels} continue_non_dominant_pixels=${continueStats.nonDominantPixels} back_non_dominant_pixels=${backStats.nonDominantPixels}`);
  } finally {
    await page.close();
    await chrome.close();
    await closeServer(server);
  }
});
