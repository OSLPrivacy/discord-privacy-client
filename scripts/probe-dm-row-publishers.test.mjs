import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const repoRoot = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const probeScript = path.join(repoRoot, 'scripts', 'probe-dm-row-publishers.mjs');

function page(title, body) {
  return `<!doctype html>
    <html>
      <head>
        <meta charset="utf-8">
        <title>${title}</title>
        <style>
          body { margin: 0; font-family: sans-serif; }
          main { width: 720px; }
          li, article { display: flex; gap: 8px; min-height: 28px; padding: 4px; }
          img { width: 24px; height: 24px; }
          .sr-only { position: absolute; left: -10000px; top: auto; width: 1px; height: 1px; overflow: hidden; }
          .meta, .bubble, .actions { min-width: 40px; min-height: 20px; }
        </style>
      </head>
      <body>${body}</body>
    </html>`;
}

function discordFixture() {
  const rows = Array.from({ length: 10 }, (_, index) => {
    const n = index + 1;
    const author = index % 2 === 0 ? 'ada' : 'grace';
    const authorId = index % 2 === 0 ? '111111111111111111' : '222222222222222222';
    return `<li
        id="chat-messages-9000-${n}"
        role="article"
        aria-label="Message from ${author} row ${n}"
        data-list-item-id="chat-messages___9000-${n}"
        data-author-id="${authorId}"
        data-author-username="${author}">
        <img alt="${author} avatar" src="https://cdn.discordapp.com/avatars/${authorId}/avatar${n}.webp">
        <div class="meta"><h3 class="username">${author}</h3><a href="https://discord.com/users/${authorId}">profile</a></div>
        <div class="bubble" role="group" aria-selected="${index === 0 ? 'true' : 'false'}">control ${n}</div>
        <div class="sr-only">screen reader message author ${author}</div>
      </li>`;
  }).join('\n');
  return page('Discord open DM fixture', `<main aria-label="Messages"><ol role="list">${rows}</ol></main>`);
}

function xFixture() {
  const rows = Array.from({ length: 10 }, (_, index) => {
    const n = index + 1;
    const sender = index % 2 === 0 ? 'alice_osl' : 'bob_osl';
    const userId = index % 2 === 0 ? '123456789012345678' : '987654321098765432';
    return `<div
        data-testid="cellInnerDiv"
        data-item-id="dm-${n}"
        role="listitem"
        aria-label="Direct message from @${sender} row ${n}"
        aria-selected="${index === 0 ? 'true' : 'false'}">
        <article data-testid="conversationMessage" data-sender-id="${userId}" aria-describedby="x-sr-${n}">
          <img alt="@${sender} profile image" src="https://pbs.twimg.com/profile_images/${userId}/avatar_${n}_normal.jpg">
          <div class="bubble" role="group">
            <a href="https://x.com/${sender}" aria-label="@${sender} profile">@${sender}</a>
            <span id="x-sr-${n}" class="sr-only">screen reader says @${sender} sent row ${n}</span>
          </div>
          <div class="actions" role="button" aria-haspopup="menu" aria-label="Message actions"></div>
        </article>
      </div>`;
  }).join('\n');
  return page('X open DM fixture', `<main aria-label="Conversation"><section role="list">${rows}</section></main>`);
}

function signedOutFixture() {
  return page('X signed-out fixture', `<main>
    <h1>Sign in to X</h1>
    <label>Phone, email, or username <input name="session[username_or_email]"></label>
    <label>Password <input type="password"></label>
    <a href="/i/flow/login">Log in</a>
  </main>`);
}

async function serveFixture() {
  const bodies = {
    '/discord': discordFixture(),
    '/x': xFixture(),
    '/signed-out': signedOutFixture(),
  };
  const server = createServer((request, response) => {
    const body = bodies[new URL(request.url, 'http://127.0.0.1').pathname] || page('missing', '<p>missing</p>');
    response.writeHead(200, {
      connection: 'close',
      'content-type': 'text/html; charset=utf-8',
    });
    response.end(body);
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  return server;
}

function runProbe(args, { expectCode = 0 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [probeScript, ...args], {
      cwd: repoRoot,
      env: process.env,
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk.toString('utf8'); });
    child.stderr.on('data', (chunk) => { stderr += chunk.toString('utf8'); });
    child.once('error', reject);
    child.once('exit', (code) => {
      if (code !== expectCode) {
        reject(new Error(`expected exit ${expectCode}, got ${code}\nstdout:\n${stdout}\nstderr:\n${stderr}`));
      } else {
        resolve({ stdout, stderr, code });
      }
    });
  });
}

test('TASK 4080 probes Discord control, X rows, and signed-out refusal', async (t) => {
  const server = await serveFixture();
  const { port } = server.address();
  t.after(async () => {
    server.closeAllConnections();
    await new Promise((resolve) => server.close(resolve));
  });

  const discord = await runProbe([
    '--surface', 'discord',
    '--url', `http://127.0.0.1:${port}/discord`,
    '--limit', '10',
    '--read-date', '2026-08-07',
  ]);
  const discordLines = discord.stdout.trim().split('\n');
  assert.equal(discordLines[0], '10 discord_who_wrote_it_rows_of_10');
  assert.equal(discordLines[1], 'discord_rows_with_no_line 0');
  assert.equal(discordLines.filter((line) => line.startsWith('discord_row ')).length, 10);

  const x = await runProbe([
    '--surface', 'x',
    '--url', `http://127.0.0.1:${port}/x`,
    '--limit', '10',
    '--read-date', '2026-08-07',
  ]);
  const xLines = x.stdout.trim().split('\n');
  assert.equal(xLines[0], 'x_page_read_date 2026-08-07');
  assert.equal(xLines.filter((line) => line.startsWith('x_row ')).length, 10);
  assert.equal(xLines.at(-1), 'x_rows_with_no_line 0');
  for (const field of [
    'test=',
    'who=',
    'roles_states=',
    'parents=',
    'boxes_beside=',
    'picture=',
    'account_links=',
    'screen_reader=',
  ]) {
    assert.match(x.stdout, new RegExp(field));
  }

  const signedOut = await runProbe([
    '--surface', 'x',
    '--url', `http://127.0.0.1:${port}/signed-out`,
    '--limit', '10',
    '--read-date', '2026-08-07',
  ], { expectCode: 1 });
  assert.match(signedOut.stderr, /signed-out login controls present/);
});
