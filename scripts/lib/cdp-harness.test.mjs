import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';

import { launchChrome } from './cdp-harness.mjs';

function serveFixture() {
  const server = createServer((_request, response) => {
    response.writeHead(200, {
      'connection': 'close',
      'content-type': 'text/html; charset=utf-8',
    });
    response.end(`<!doctype html>
      <title>CDP fixture</title>
      <style>.known-rule { color: rgb(12, 34, 56); }</style>
      <p class="known-rule">fixture</p>`);
  });
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => resolve(server));
  });
}

test('CDP harness evaluates a computed style from a fixture page', async (t) => {
  const server = await serveFixture();
  const { port } = server.address();
  const chrome = await launchChrome();
  t.after(async () => {
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });

  const page = await chrome.openPage();
  t.after(() => page.close());
  await page.navigate(`http://127.0.0.1:${port}/`);

  assert.equal(
    await page.evaluate("getComputedStyle(document.querySelector('.known-rule')).color"),
    'rgb(12, 34, 56)',
  );
});
