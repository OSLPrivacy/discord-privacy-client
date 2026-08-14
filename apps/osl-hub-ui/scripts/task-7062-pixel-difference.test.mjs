import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { createServer } from "node:http";
import test from "node:test";
import { deflateSync } from "node:zlib";
import { PINNED_CANVAS } from "./task-7053-geometry-relationships.mjs";
import { NEUTRALISATION_MODE, measureCapturedPair, renderAndMeasure } from "./task-7062-pixel-difference.mjs";

function chunk(type, data) {
  const header = Buffer.alloc(8);
  header.writeUInt32BE(data.length, 0);
  header.write(type, 4, 4, "ascii");
  return Buffer.concat([header, data, Buffer.alloc(4)]);
}

function png(changes = []) {
  const width = PINNED_CANVAS.width;
  const height = PINNED_CANVAS.height;
  const rows = Buffer.alloc((width * 4 + 1) * height);
  for (let y = 0; y < height; y += 1) {
    const start = y * (width * 4 + 1);
    rows[start] = 0;
    for (let x = 0; x < width; x += 1) {
      const pixel = start + 1 + x * 4;
      rows[pixel] = 24;
      rows[pixel + 1] = 36;
      rows[pixel + 2] = 48;
      rows[pixel + 3] = 255;
    }
  }
  for (const { x, y, rgba = [220, 230, 240, 255] } of changes) {
    const offset = y * (width * 4 + 1) + 1 + x * 4;
    rows.set(rgba, offset);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("IDAT", deflateSync(rows)), chunk("IEND", Buffer.alloc(0))]);
}

function capture(image, overrides = {}) {
  return {
    png: image,
    viewport: { ...PINNED_CANVAS },
    devicePixelRatio: 1,
    region: { x: 0, y: 0, ...PINNED_CANVAS },
    cropping: false,
    scale: 1,
    downscaled: false,
    blurRadius: 0,
    tolerance: 0,
    recordedPixelCount: PINNED_CANVAS.width * PINNED_CANVAS.height,
    neutralisation: { applied: true, mode: NEUTRALISATION_MODE, digest: "same-semantic-plan" },
    ...overrides,
  };
}

function pair(design, build, overrides = {}) {
  return { design: capture(design, overrides.design), build: capture(build, overrides.build), buildId: "ui-build-7062", designPage: "Conversation.dc.html", buildRoute: "inbox/conversation" };
}

function expectKnob(input, knob) {
  assert.throws(() => measureCapturedPair(input), new RegExp(knob, "u"));
}

test("TASK 7062 exact pixels are deterministic and record every measurement knob", () => {
  const design = png([{ x: 0, y: 0 }]);
  const build = png([{ x: 0, y: 0 }, { x: 10, y: 10 }]);
  const first = measureCapturedPair(pair(design, build));
  const second = measureCapturedPair(pair(design, build));
  assert.equal(first.percent_different, second.percent_different);
  assert.equal(first.percent_different, Number((100 / (1280 * 800)).toFixed(6)));
  assert.equal(first.compared_pixel_count, 1_024_000);
  assert.deepEqual(first.viewport, { width: 1280, height: 800 });
  assert.equal(first.device_pixel_ratio, 1);
  assert.equal(first.build, "ui-build-7062");
  assert.equal(first.design_page, "Conversation.dc.html");
  assert.equal(first.build_route, "inbox/conversation");
  assert.equal(first.design_image_digest, createHash("sha256").update(design).digest("hex"));
  assert.equal(first.comparison.tolerance, 0);
  console.log(`TASK7062_DETERMINISTIC percent=${first.percent_different} pixels=${first.compared_pixel_count} dpr=${first.device_pixel_ratio} build=${first.build}`);
});

test("TASK 7062 refuses blank, mismatched-DPR, and incomplete-region captures by name", () => {
  const ink = png([{ x: 1, y: 1 }]);
  const blank = png();
  expectKnob(pair(blank, ink), "blank capture: design");
  expectKnob(pair(ink, blank), "blank capture: build");
  assert.throws(() => measureCapturedPair(pair(ink, ink, { build: { devicePixelRatio: 2 } })), /device pixel ratio: design=1 build=2/u);
  assert.throws(() => measureCapturedPair(pair(ink, ink, { design: { recordedPixelCount: 1_023_999 } })), /region missing: design compared pixel count is 1023999; the full pinned region needs 1024000/u);
});

test("TASK 7062 neutralised demo pixels contribute zero while a control pixel contributes more than zero", () => {
  // The first pair models captures after the same semantic demo slot became
  // the canonical DEMO token on both rendered sides.  The control mutation is
  // deliberately outside that slot and remains an exact pixel defect.
  const neutralDemoDesign = png([{ x: 100, y: 100 }]);
  const neutralDemoBuild = png([{ x: 100, y: 100 }]);
  const demoOnly = measureCapturedPair(pair(neutralDemoDesign, neutralDemoBuild));
  const changedControl = measureCapturedPair(pair(neutralDemoDesign, png([{ x: 100, y: 100 }, { x: 700, y: 400 }])));
  assert.equal(demoOnly.percent_different, 0);
  assert.ok(changedControl.percent_different > 0);
  console.log(`TASK7062_NEUTRAL_DEMO percent=${demoOnly.percent_different} TASK7062_CONTROL_CHANGE percent=${changedControl.percent_different}`);
});

test("TASK 7062 rejects every anti-faking knob instead of scoring it", () => {
  const ink = png([{ x: 1, y: 1 }]);
  for (const [knob, overrides] of [
    ["cropping", { design: { cropping: true } }],
    ["downscaling", { design: { scale: 0.5, downscaled: true } }],
    ["blurring", { design: { blurRadius: 1 } }],
    ["tolerance band", { design: { tolerance: 1 } }],
    ["one-sided neutralisation", { build: { neutralisation: { applied: true, mode: NEUTRALISATION_MODE, digest: "other-side" } } }],
    ["neutralisation", { design: { neutralisation: null } }],
    ["pinned canvas", { design: { viewport: { width: 1279, height: 800 } } }],
    ["capture", { design: { png: Buffer.alloc(0) } }],
  ]) expectKnob(pair(ink, ink, overrides), knob);
  expectKnob({ design: capture(ink), build: capture(ink), buildId: "" }, "recorded metadata");
});

test("TASK 7062 renders the design page and built route, neutralises both, then captures full canvases", async () => {
  const design = `<!doctype html><html><head><style>html,body{margin:0;width:1280px;height:800px;background:#17222d;color:#eef5fa;font:24px sans-serif}main{padding:40px}button{font:24px sans-serif}</style></head><body><main><h1>Conversation</h1><section role="list" aria-label="Messages"><div role="listitem">Avery says hello at 09:41</div></section><button>Send reply</button></main></body></html>`;
  const build = design.replace("Avery says hello at 09:41", "Bryn replies from a very different place at 18:02");
  const server = createServer((request, response) => {
    response.setHeader("content-type", "text/html; charset=utf-8");
    response.end(request.url === "/design" ? design : build);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  const base = `http://127.0.0.1:${address.port}`;
  try {
    const options = { designUrl: `${base}/design`, buildUrl: `${base}/build`, buildId: "built-route-fixture-7062", designPage: "Conversation.dc.html", buildRoute: "inbox/conversation" };
    const first = await renderAndMeasure(options);
    const second = await renderAndMeasure(options);
    assert.equal(first.percent_different, 0, "demo-only text was canonicalised before screenshots");
    assert.equal(second.percent_different, first.percent_different, "same pair is deterministic");
    assert.equal(first.compared_pixel_count, 1_024_000);
    assert.equal(first.neutralisation.applied_to.join(","), "design,build");
    console.log(`TASK7062_RENDERED percent=${first.percent_different} design_sha256=${first.design_image_digest} build_sha256=${first.build_image_digest} viewport=1280x800 dpr=1 pixels=${first.compared_pixel_count} build=${first.build}`);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});
