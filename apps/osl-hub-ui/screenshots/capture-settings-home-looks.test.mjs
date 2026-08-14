import assert from "node:assert/strict";
import test from "node:test";
import zlib from "node:zlib";

import {
  LOOKS,
  REQUIRED_CONTROLS,
  pngMeanLuma,
  validateSettingsHomeLookCapture,
  validateSettingsHomeLooks,
} from "./capture-settings-home-looks.mjs";
import { FIXED_VIEWPORT } from "./capture-settings-home.mjs";

// Minimal valid-enough RGBA PNG (filter 0 rows, dummy CRCs — the decoder
// reads chunk layout, not checksums) filled with one solid color.
function solidPng(width, height, [r, g, b]) {
  const chunk = (kind, data) => {
    const out = Buffer.alloc(12 + data.length);
    out.writeUInt32BE(data.length, 0);
    out.write(kind, 4, "ascii");
    data.copy(out, 8);
    return out;
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  const raster = Buffer.alloc(height * (1 + width * 4));
  for (let y = 0; y < height; y += 1) {
    const row = y * (1 + width * 4);
    for (let x = 0; x < width; x += 1) {
      raster[row + 1 + x * 4] = r;
      raster[row + 2 + x * 4] = g;
      raster[row + 3 + x * 4] = b;
      raster[row + 4 + x * 4] = 255;
    }
  }
  return Buffer.concat([
    Buffer.from("89504e470d0a1a0a", "hex"),
    chunk("IHDR", ihdr),
    chunk("IDAT", zlib.deflateSync(raster)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function completeLook(look) {
  return {
    look,
    platform: "linux",
    imageText: `Settings ${REQUIRED_CONTROLS.join(" ")}`,
    axNames: ["Settings", ...REQUIRED_CONTROLS.map((control) => `${control} one-line explanation`)],
    png: {
      ...FIXED_VIEWPORT,
      bytes: 20_000,
      distinctColors: 80,
      sha256: (look === "light" ? "a" : "b").repeat(64),
      meanLuma: look === "light" ? 231.4 : 24.9,
    },
  };
}

function completePair() {
  return { dark: completeLook("dark"), light: completeLook("light") };
}

test("TASK0718 pngMeanLuma reads real pixels: white is bright, near-black is dark", () => {
  assert.ok(pngMeanLuma(solidPng(8, 8, [255, 255, 255])) > 250);
  assert.ok(pngMeanLuma(solidPng(8, 8, [10, 10, 10])) < 15);
});

test("TASK0718 accepts a Linux light+dark pair carrying Settings and all eight controls", () => {
  const checked = validateSettingsHomeLooks(completePair());
  for (const look of LOOKS) {
    assert.equal(checked[look].platform, "linux");
    assert.deepEqual(checked[look].controls, [
      "Account", "Apps", "Whitelisting", "Scrub", "Cleanup", "Notifications", "Appearance", "About",
    ]);
  }
});

test("TASK0718 goes red when a control is missing from the screen tree", () => {
  const pair = completePair();
  pair.light.axNames = pair.light.axNames.filter((name) => !name.includes("Whitelisting"));
  assert.throws(() => validateSettingsHomeLooks(pair), /light: screen tree is missing the control Whitelisting/u);
});

test("TASK0718 goes red when a control is missing from the visible text", () => {
  const pair = completePair();
  pair.dark.imageText = pair.dark.imageText.replace("Cleanup", "");
  assert.throws(() => validateSettingsHomeLooks(pair), /dark: visible text is missing the control Cleanup/u);
});

test("TASK0718 goes red when the title Settings is missing", () => {
  const pair = completePair();
  pair.dark.axNames = pair.dark.axNames.filter((name) => name !== "Settings");
  assert.throws(() => validateSettingsHomeLooks(pair), /dark: screen tree is missing the title Settings/u);
});

test("TASK0718 goes red when a PNG is blank or nearly blank", () => {
  const pair = completePair();
  pair.light.png.distinctColors = 4;
  assert.throws(() => validateSettingsHomeLooks(pair), /light: PNG is blank or nearly blank/u);
});

test("TASK0718 goes red when the light capture is not actually light", () => {
  const pair = completePair();
  pair.light.png.meanLuma = 24.9;
  assert.throws(() => validateSettingsHomeLooks(pair), /light: pixels are not light, mean luminance 24.9/u);
});

test("TASK0718 goes red when the dark capture is not actually dark", () => {
  const pair = completePair();
  pair.dark.png.meanLuma = 231.4;
  assert.throws(() => validateSettingsHomeLooks(pair), /dark: pixels are not dark, mean luminance 231.4/u);
});

test("TASK0718 goes red when both looks captured the same pixels", () => {
  const pair = completePair();
  pair.dark.png.sha256 = pair.light.png.sha256;
  assert.throws(() => validateSettingsHomeLooks(pair), /pixel-identical/u);
});

test("TASK0718 goes red when a look is missing or foreign", () => {
  assert.throws(() => validateSettingsHomeLooks({ dark: completeLook("dark") }), /missing light capture/u);
  const stray = completeLook("dark");
  stray.look = "sepia";
  assert.throws(() => validateSettingsHomeLookCapture(stray), /unknown look "sepia"/u);
});

test("TASK0718 goes red when the capture is not from Linux", () => {
  const pair = completePair();
  pair.light.platform = "win32";
  assert.throws(() => validateSettingsHomeLooks(pair), /light: screenshot must be captured on Linux, got win32/u);
});
