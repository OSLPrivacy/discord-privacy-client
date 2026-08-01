import assert from "node:assert/strict";
import { createServer } from "node:http";
import test from "node:test";
import { bootIntervals, emittedBootTimeline, measureBootTimeline } from "./measure-osl-hub-boot.mjs";

async function fixtureUrl() {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(`<!doctype html><title>boot fixture</title><div id="app">Visible content</div><script type="module">
      setTimeout(() => {
        document.querySelector("#app").innerHTML = "Loading";
        requestAnimationFrame(() => { document.querySelector("#app").innerHTML = '<h1 id="route-heading">Home</h1>'; });
      }, 100);
    </script>`);
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const { port } = server.address();
  return { server, url: `http://127.0.0.1:${port}/` };
}

test("TU-06 emits all four ordered boot intervals", () => {
  const output = emittedBootTimeline({
    windowVisibleMs: 0,
    firstPaintMs: 12,
    firstContentfulPaintMs: 18,
    bootstrapAssignmentMs: 43,
    routePaintedMs: 71,
  });

  assert.deepEqual(output.intervals_ms, {
    window_visible_to_first_paint_ms: 12,
    first_paint_to_first_contentful_paint_ms: 6,
    first_contentful_paint_to_bootstrap_assignment_ms: 25,
    bootstrap_assignment_to_route_painted_ms: 28,
  });
});

test("TU-06 observes four ordered intervals in a rendered page", async () => {
  const { server, url } = await fixtureUrl();
  try {
    const output = await measureBootTimeline({ url, timeoutMs: 10_000 });
    assert.deepEqual(Object.keys(output.intervals_ms), [
      "window_visible_to_first_paint_ms",
      "first_paint_to_first_contentful_paint_ms",
      "first_contentful_paint_to_bootstrap_assignment_ms",
      "bootstrap_assignment_to_route_painted_ms",
    ]);
    assert.deepEqual(Object.values(output.intervals_ms).map((value) => Number.isFinite(value) && value >= 0), [true, true, true, true]);
  } finally {
    await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()));
  }
});

test("TU-06 rejects an out-of-order boot mark", () => {
  assert.throws(
    () => bootIntervals({
      windowVisibleMs: 0,
      firstPaintMs: 12,
      firstContentfulPaintMs: 18,
      bootstrapAssignmentMs: 17,
      routePaintedMs: 71,
    }),
    /bootstrapAssignmentMs preceded firstContentfulPaintMs/u,
  );
});
