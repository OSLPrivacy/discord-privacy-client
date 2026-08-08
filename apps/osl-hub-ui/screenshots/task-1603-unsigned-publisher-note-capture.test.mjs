// TASK 1603 - render the unsigned / unknown-publisher install note in a real
// browser and read the finish line off the live page.
//
// Nothing here is asserted against a string in a source file where it can be
// avoided. The installer is built by the repository's own installer step
// (`scripts/installer_recipe.py`, the same one task 1602 breaks on purpose),
// its SHA-256 is measured twice -- once with node's crypto, once with
// `sha256sum` -- and, where Windows PowerShell is reachable, the exact
// `Get-FileHash` command the note prints is run against that file and its
// output compared with the value the note shows.
//
// The promise scan runs in the page, over `document.body.innerText`, so "zero
// promises that Windows will trust it" is a number taken from the rendered
// words rather than from the module's inputs.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { createServer } from "node:http";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { inflateSync } from "node:zlib";
import test from "node:test";
import * as esbuild from "esbuild";
import { launchChrome } from "../../../scripts/lib/cdp-harness.mjs";

const ROOT = resolve(new URL("..", import.meta.url).pathname);
const REPO_ROOT = resolve(ROOT, "..", "..");
const ARTIFACT_DIR = join(ROOT, "screenshots", "evidence");
const PNG_PATH = join(ARTIFACT_DIR, "task-1603-unsigned-publisher-note-1000x900.png");
const FULL_PNG_PATH = join(ARTIFACT_DIR, "task-1603-unsigned-publisher-note-full.png");
const AX_PATH = join(ARTIFACT_DIR, "task-1603-unsigned-publisher-note-1000x900.ax.json");
const WINDOW = Object.freeze({ width: 1000, height: 900 });
const INSTALLER_VERSION = "2.0.0";
const INSTALLER_NAME = `OSL-${INSTALLER_VERSION}.exe`;
const BUILD_FINGERPRINT = "MAPLE-4172";
const POWERSHELL = "/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe";

/** Build a real installer file with the repository's own installer step. */
function buildInstaller() {
  const workDir = mkdtempSync(join(tmpdir(), "osl-task-1603-"));
  const inputDir = join(workDir, "input");
  const outputDir = join(workDir, "output");
  mkdirSync(inputDir, { recursive: true });
  writeFileSync(
    join(inputDir, "recipe.json"),
    `${JSON.stringify({ version: INSTALLER_VERSION, build: { fingerprint: BUILD_FINGERPRINT } }, null, 2)}\n`,
  );
  const recipeOutput = execFileSync(
    "python3",
    [join(REPO_ROOT, "scripts", "installer_recipe.py"), "--input-dir", inputDir, "--output-dir", outputDir],
    { encoding: "utf8" },
  ).trim();
  const installerPath = join(outputDir, INSTALLER_NAME);
  const bytes = readFileSync(installerPath);
  assert.ok(bytes.length > 0, "the installer step produced an empty file");
  return { outputDir, installerPath, bytes, recipeOutput };
}

/** The exact command the note prints, run for real. Returns null when Windows is out of reach. */
function powershellGetFileHash(command, outputDir) {
  let windowsDir;
  try {
    windowsDir = execFileSync("wslpath", ["-w", outputDir], { encoding: "utf8" }).trim();
    execFileSync(POWERSHELL, ["-NoProfile", "-Command", "exit 0"], { stdio: "ignore" });
  } catch {
    return null;
  }
  const script = `Set-Location -LiteralPath '${windowsDir}'; (${command}).Hash`;
  return execFileSync(POWERSHELL, ["-NoProfile", "-Command", script], { encoding: "utf8" }).trim();
}

async function pageScript(sha256) {
  const outFile = join(tmpdir(), `osl-task-1603-fixture-${process.pid}-${Date.now()}.js`);
  await esbuild.build({
    stdin: {
      contents: `
        import {
          renderUnsignedPublisherNote,
          unsignedPublisherNoteRelease,
          unsignedPublisherNotePromises,
          NEITHER_IS_PROOF_SENTENCE,
        } from "${join(ROOT, "src", "unsigned-publisher-note.ts")}";
        const release = unsignedPublisherNoteRelease(${JSON.stringify(INSTALLER_NAME)}, ${JSON.stringify(sha256)});
        renderUnsignedPublisherNote(document.getElementById("app"), release);
        window.task1603 = {
          promises: (text) => unsignedPublisherNotePromises(text),
          notProofSentence: NEITHER_IS_PROOF_SENTENCE,
        };
      `,
      resolveDir: ROOT,
      sourcefile: "task-1603-fixture-entry.js",
      loader: "js",
    },
    bundle: true,
    platform: "browser",
    format: "iife",
    outfile: outFile,
    loader: { ".css": "empty", ".svg": "text", ".png": "dataurl" },
    logLevel: "silent",
  });
  return readFileSync(outFile, "utf8");
}

function fixtureHtml(script) {
  const css = [
    readFileSync(join(ROOT, "src", "styles.css"), "utf8"),
    readFileSync(join(ROOT, "src", "unsigned-publisher-note.css"), "utf8"),
    "html, body { height: 100%; margin: 0; background: var(--bg); }",
  ].join("\n");
  return `<!doctype html>
    <html lang="en">
      <head>
        <meta charset="utf-8">
        <title>Install note</title>
        <style>${css}</style>
      </head>
      <body>
        <div id="app"></div>
        <script>${script}</script>
      </body>
    </html>`;
}

async function fixtureServer(html) {
  const server = createServer((request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  await new Promise((ready) => server.listen(0, "127.0.0.1", ready));
  const { port } = server.address();
  return { server, url: `http://127.0.0.1:${port}/` };
}

function closeServer(server) {
  return new Promise((done, fail) => server.close((error) => (error ? fail(error) : done())));
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

function pixelStats(image) {
  const colors = new Map();
  for (let index = 0; index < image.width * image.height; index += 1) {
    const offset = index * 4;
    const key = `${image.pixels[offset]},${image.pixels[offset + 1]},${image.pixels[offset + 2]}`;
    colors.set(key, (colors.get(key) ?? 0) + 1);
  }
  const dominant = Math.max(...colors.values());
  return { uniqueColors: colors.size, nonDominantPixels: image.width * image.height - dominant };
}

/** Everything the finish line needs, read from the live page. */
const PROBE = `(() => {
  const note = document.querySelector("#unsigned-publisher-note");
  const text = document.body.innerText;
  const sentences = text.split(/(?<=[.!?])\\s+/).map((s) => s.trim()).filter(Boolean);
  return {
    warnings: Array.from(document.querySelectorAll("[data-warning]")).map((el) => el.getAttribute("data-warning")),
    warningNames: Array.from(document.querySelectorAll("[data-warning-name]")).map((el) => el.textContent.trim()),
    notProof: document.querySelector("#unsigned-publisher-note-not-proof").textContent.trim(),
    notProofMatchesModule: document.querySelector("#unsigned-publisher-note-not-proof").textContent.trim() === window.task1603.notProofSentence,
    notProofSentenceCount: document.querySelector("#unsigned-publisher-note-not-proof").textContent.trim().split(/(?<=[.!?])\\s+/).filter(Boolean).length,
    proofClaimCount: sentences.filter((s) => /neither/i.test(s) && /proof/i.test(s) && /unsafe/i.test(s)).length,
    command: document.querySelector("#unsigned-publisher-note-command").textContent.trim(),
    expected: document.querySelector("#unsigned-publisher-note-expected").textContent.trim(),
    checksumLine: document.querySelector("#unsigned-publisher-note-checksum-line").textContent.trim(),
    dataExpected: note.getAttribute("data-expected-sha256"),
    promisesFromPage: window.task1603.promises(text),
    promiseCountAttribute: note.getAttribute("data-promise-count"),
    buttons: document.querySelectorAll("button").length,
    links: document.querySelectorAll("a").length,
    visibleText: text,
  };
})()`;

test("TASK 1603 install note names both warnings, gives the checksum command and value, and promises nothing", async () => {
  mkdirSync(ARTIFACT_DIR, { recursive: true });
  const log = [];

  // --- the installer, and its measured checksum -------------------------
  const installer = buildInstaller();
  const sha256 = createHash("sha256").update(installer.bytes).digest("hex");
  const sha256sumOutput = execFileSync("sha256sum", [installer.installerPath], { encoding: "utf8" }).trim();
  assert.equal(sha256sumOutput.split(/\s+/)[0], sha256, "node and sha256sum disagree about the installer hash");
  log.push(
    `TASK1603_INSTALLER recipe="${installer.recipeOutput}" name=${INSTALLER_NAME}`
    + ` size_bytes=${installer.bytes.length} sha256=${sha256} sha256sum="${sha256sumOutput}"`,
  );

  const { server, url } = await fixtureServer(fixtureHtml(await pageScript(sha256)));
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
    const probe = await page.evaluate(PROBE);

    // --- 1: both warnings are named --------------------------------------
    assert.deepEqual(probe.warnings, ["unsigned", "unknown-publisher"], "the note does not carry both warnings");
    assert.equal(probe.warningNames.length, 2);
    assert.ok(/Windows protected your PC/u.test(probe.visibleText), "the unsigned-app warning is not named");
    assert.ok(/Unknown publisher/u.test(probe.visibleText), "the unknown-publisher warning is not named");
    assert.ok(/unsigned-app warning/u.test(probe.visibleText), "the unsigned warning is not called what it is");
    assert.ok(/unknown-publisher warning/u.test(probe.visibleText), "the unknown-publisher warning is not called what it is");
    assert.ok(/Microsoft Defender SmartScreen/u.test(probe.visibleText));
    assert.ok(/User Account Control/u.test(probe.visibleText));
    log.push(`TASK1603_WARNINGS ids=${probe.warnings.join(",")} names=${probe.warningNames.map((name) => JSON.stringify(name)).join(" | ")}`);

    // --- 2: one sentence, and only one, says neither is proof -------------
    assert.equal(probe.proofClaimCount, 1, "the not-proof claim is missing or said more than once");
    assert.equal(probe.notProofSentenceCount, 1, "the not-proof claim is more than one sentence");
    assert.equal(probe.notProofMatchesModule, true);
    assert.match(probe.notProof, /^Neither warning is proof that this installer is unsafe;/u);
    log.push(`TASK1603_NOT_PROOF sentences=${probe.notProofSentenceCount} occurrences_in_note=${probe.proofClaimCount} text="${probe.notProof}"`);

    // --- 3: the exact command and the exact expected value ----------------
    const expectedCommand = `Get-FileHash -Algorithm SHA256 -LiteralPath .\\${INSTALLER_NAME}`;
    assert.equal(probe.command, expectedCommand, "the note does not print the exact checksum command");
    assert.equal(probe.expected, sha256.toUpperCase(), "the note's expected value is not the installer's hash");
    assert.equal(probe.checksumLine, `${sha256}  ${INSTALLER_NAME}`);
    assert.equal(probe.dataExpected, sha256);
    log.push(`TASK1603_CHECKSUM command="${probe.command}" expected_value=${probe.expected} checksum_list_line="${probe.checksumLine}"`);

    // The command, run for real against that installer.
    const powershellHash = powershellGetFileHash(probe.command, installer.outputDir);
    if (powershellHash === null) {
      log.push("TASK1603_POWERSHELL skipped=windows_powershell_unreachable");
    } else {
      assert.equal(powershellHash, probe.expected, "PowerShell's Get-FileHash disagrees with the note's expected value");
      log.push(`TASK1603_POWERSHELL command="${probe.command}" get_filehash_output=${powershellHash} note_expected=${probe.expected} match=${powershellHash === probe.expected}`);
    }

    // --- 4: zero promises that Windows will trust it ----------------------
    assert.deepEqual(probe.promisesFromPage, [], `the note promises Windows will come round: ${probe.promisesFromPage.join(",")}`);
    assert.equal(probe.promiseCountAttribute, "0");
    log.push(`TASK1603_PROMISES scanned_chars=${probe.visibleText.length} promises_found=${probe.promisesFromPage.length} patterns_checked=9`);

    // The scanner is not decoration: a promise added to these exact rendered
    // words is caught.
    const sabotage = await page.evaluate(
      `window.task1603.promises(document.body.innerText + "\\nWindows will trust OSL after a few installs.")`,
    );
    assert.ok(sabotage.includes("windows-will-trust"), "the promise scanner cannot go red");
    const restored = await page.evaluate(`window.task1603.promises(document.body.innerText)`);
    assert.deepEqual(restored, [], "the note is not clean again after the sabotage");
    log.push(`TASK1603_PROMISE_SCANNER_SABOTAGE with_promise=${sabotage.join(",")} promise_removed=${restored.length}`);

    // --- the note is text; there is nothing on it to press ----------------
    assert.equal(probe.buttons, 0, "the install note carries a button");
    assert.equal(probe.links, 0, "the install note carries a link");

    const axTree = await page.send("Accessibility.getFullAXTree");
    const names = axTree.nodes
      .map((node) => ({ role: node.role?.value ?? "", name: node.name?.value ?? "" }))
      .filter((node) => node.name);
    writeFileSync(AX_PATH, JSON.stringify(names, null, 2));

    const png = await page.screenshot({ fromSurface: true });
    writeFileSync(PNG_PATH, png);
    const image = readPng(png);
    assert.deepEqual({ width: image.width, height: image.height }, WINDOW);
    const stats = pixelStats(image);
    assert.ok(stats.nonDominantPixels > 5_000, `page barely painted: ${JSON.stringify(stats)}`);
    log.push(`TASK1603_IMAGES png=${PNG_PATH} ax=${AX_PATH} window=${image.width}x${image.height} non_dominant_pixels=${stats.nonDominantPixels} ax_named_nodes=${names.length}`);

    // The note runs past 900px, and the part that runs past is the part that
    // says the check changes nothing about the warnings -- so it gets its own
    // full-height capture rather than sitting below an evidence fold.
    const noteHeight = await page.evaluate(
      `Math.ceil(document.querySelector("#unsigned-publisher-note").getBoundingClientRect().height)`,
    );
    for (const limit of ["does not remove either warning", "blocked outright", "and nothing more"]) {
      assert.ok(probe.visibleText.includes(limit), `the note does not say: ${limit}`);
    }
    await page.send("Emulation.setDeviceMetricsOverride", {
      width: WINDOW.width,
      height: noteHeight,
      deviceScaleFactor: 1,
      mobile: false,
    });
    const fullPng = await page.screenshot({ fromSurface: true });
    writeFileSync(FULL_PNG_PATH, fullPng);
    const fullImage = readPng(fullPng);
    assert.equal(fullImage.width, WINDOW.width);
    assert.ok(fullImage.height >= noteHeight, "the full capture is shorter than the note");
    log.push(`TASK1603_FULL_IMAGE png=${FULL_PNG_PATH} size=${fullImage.width}x${fullImage.height} note_height_px=${noteHeight} non_dominant_pixels=${pixelStats(fullImage).nonDominantPixels}`);

    for (const line of log) console.log(line);
  } finally {
    await page.close();
    await chrome.close();
    await closeServer(server);
  }
});
