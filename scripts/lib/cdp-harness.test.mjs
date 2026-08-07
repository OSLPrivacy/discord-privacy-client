import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';

import { launchChrome } from './cdp-harness.mjs';

function serveFixture({ title = 'CDP fixture', body = '<p class="known-rule">fixture</p>' } = {}) {
  const server = createServer((_request, response) => {
    response.writeHead(200, {
      'connection': 'close',
      'content-type': 'text/html; charset=utf-8',
    });
    response.end(`<!doctype html>
      <title>${title}</title>
      <style>.known-rule { color: rgb(12, 34, 56); }</style>
      ${body}`);
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

test('CDP harness refuses a read after the real page target closes', async (t) => {
  const server = await serveFixture({ title: 'MAPLE-4172' });
  const { port } = server.address();
  const chrome = await launchChrome();
  t.after(async () => {
    await chrome.close();
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });

  const page = await chrome.openPage();
  await page.navigate(`http://127.0.0.1:${port}/`);

  const readState = {
    successfulReadCount: 0,
    savedTitle: undefined,
  };

  async function readTitle() {
    try {
      const title = await page.evaluate('document.title');
      readState.successfulReadCount += 1;
      readState.savedTitle = title;
      return { ok: true, title };
    } catch (error) {
      return { ok: false, error: 'page unavailable', driverError: error.message };
    }
  }

  const beforeOpenReadCount = readState.successfulReadCount;
  const openRead = await readTitle();
  const afterOpenReadCount = readState.successfulReadCount;

  await page.close();
  const closedRead = await readTitle();
  const afterClosedReadCount = readState.successfulReadCount;

  assert.equal(beforeOpenReadCount, 0);
  assert.deepEqual(openRead, { ok: true, title: 'MAPLE-4172' });
  assert.equal(afterOpenReadCount, 1);
  assert.equal(closedRead.ok, false);
  assert.equal(closedRead.error, 'page unavailable');
  assert.match(closedRead.driverError, /(?:Session|Target|target|closed|detached|not found)/);
  assert.equal(readState.savedTitle, 'MAPLE-4172');
  assert.equal(afterClosedReadCount, 1);

  console.log(JSON.stringify({
    beforeOpenReadCount,
    openRead,
    afterOpenReadCount,
    closedRefusal: closedRead.error,
    savedTitle: readState.savedTitle,
    afterClosedReadCount,
  }));
});
