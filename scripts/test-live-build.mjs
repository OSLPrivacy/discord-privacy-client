import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { createServer } from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const VERIFIER = path.join(SCRIPT_DIR, 'verify-live-build.mjs');
const H25_ACCEPTANCE_COMMAND = 'test -n "$OSL_KEYSERVER_REDEMPTION_EVIDENCE" && node scripts/verify-live-build.mjs --url="$OSL_LIVE_URL" --sha="$OSL_LIVE_SHA" --branch=main --environment=production';

const commit = 'a'.repeat(40);
const shortCommit = commit.slice(0, 8);
const branch = 'main';
const environment = 'production';
const index = Buffer.from([
  '<!doctype html><html><head>',
  `<meta name="osl-build" content="${commit}">`,
  '</head><body>fixture</body></html>',
].join(''));
const success = Buffer.from([
  '<!doctype html><html><head>',
  `<meta name="osl-build" content="${commit}">`,
  '</head><body>',
  '<p data-osl-claim="pro-expiry-limitation">If this page follows an earlier completed payment, keep the receipt and activation code. Nothing renews and OSL stores no payment method. Automatic one-month expiry is not implemented, so this page does not claim when the issued licence ends. The payment record contains no message text, no conversation names, no carrier text and no recipient identity.</p>',
  '</body></html>',
].join(''));
const asset = Buffer.from('void 0;\n');
const headers = Buffer.from('/*\n  X-Content-Type-Options: nosniff\n');
const redirects = Buffer.from('/old / 301\n');
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');

const baseBuild = {
  schema_version: 2,
  commit,
  short_commit: shortCommit,
  branch,
  environment,
  dirty: false,
  built_at: '2026-07-27T00:00:00.000Z',
  manifest_version: 5,
  inputs: {
    '.assetsignore': digest(Buffer.from('scripts/\n')),
    '_headers': digest(headers),
    '_redirects': digest(redirects),
    'data/at-rest-census.json': digest(Buffer.from('{"schema_version":1}\n')),
    'data/pricing.json': digest(Buffer.from('{"manifest_version":5}\n')),
    'data/public-surface-manifest.json': digest(Buffer.from('{"schema_version":1}\n')),
    'wrangler.jsonc': digest(Buffer.from('{}\n')),
  },
  artifact_files: {
    '_headers': digest(headers),
    '_redirects': digest(redirects),
    'assets/main.js': digest(asset),
    'index.html': digest(index),
    'success.html': digest(success),
  },
  files: {
    'assets/main.js': digest(asset),
    'index.html': digest(index),
    'success.html': digest(success),
  },
};

let mode = 'positive';
let port;
let passed = 0;

function successBytes() {
  if (mode === 'missing-pro-expiry-limitation') {
    return Buffer.from(success.toString().replace(
      /<p data-osl-claim="pro-expiry-limitation">[\s\S]*?<\/p>/,
      '<p>Automatic one-month expiry is implemented.</p>',
    ));
  }
  if (mode === 'misstated-pro-expiry-limitation') {
    return Buffer.from(success.toString().replace(
      'Automatic one-month expiry is not implemented',
      'Automatic one-month expiry is implemented',
    ));
  }
  if (mode === 'preview-missing-pro-expiry-limitation') {
    return Buffer.from(success.toString().replace(
      /<p data-osl-claim="pro-expiry-limitation">[\s\S]*?<\/p>/,
      '<p>Preview builds do not publish the production checkout promotion proof.</p>',
    ));
  }
  return success;
}

function buildJson() {
  const build = structuredClone(baseBuild);
  const currentSuccess = successBytes();
  build.artifact_files['success.html'] = digest(currentSuccess);
  build.files['success.html'] = digest(currentSuccess);
  if (mode === 'preview' || mode === 'preview-missing-pro-expiry-limitation') {
    build.environment = 'preview';
    build.branch = 'preview-branch';
  }
  if (mode === 'missing-success') {
    delete build.artifact_files['success.html'];
    delete build.files['success.html'];
  }
  return build;
}

const server = createServer((request, response) => {
  const url = new URL(request.url, `http://127.0.0.1:${port}`);
  if (url.pathname === '/build.json') {
    response.writeHead(200, { 'Cache-Control': 'no-store, max-age=0' });
    response.end(JSON.stringify(buildJson()));
    return;
  }
  if (url.pathname === '/' || url.pathname === '/index.html') {
    response.end(index);
    return;
  }
  if (url.pathname === '/success.html') {
    response.end(successBytes());
    return;
  }
  if (url.pathname === '/assets/main.js') {
    response.end(asset);
    return;
  }
  response.writeHead(404).end('missing');
});

function runVerifier({
  evidence = 'fixture-redemption-evidence',
  verifierBranch = branch,
  verifierEnvironment = environment,
} = {}) {
  return new Promise((resolve, reject) => {
    const env = { ...process.env };
    if (evidence === undefined) delete env.OSL_KEYSERVER_REDEMPTION_EVIDENCE;
    else env.OSL_KEYSERVER_REDEMPTION_EVIDENCE = evidence;
    const child = spawn(process.execPath, [
      VERIFIER,
      `--url=http://127.0.0.1:${port}`,
      `--sha=${commit}`,
      `--branch=${verifierBranch}`,
      `--environment=${verifierEnvironment}`,
    ], { encoding: 'utf8', env });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.stderr.on('data', (chunk) => { stderr += chunk; });
    child.once('error', reject);
    child.once('exit', (code) => resolve({ code, output: `${stdout}${stderr}` }));
  });
}

async function check(name, selectedMode, expectedCode, phrase, options) {
  mode = selectedMode;
  const result = await runVerifier(options);
  if (result.code !== expectedCode || (phrase && !result.output.includes(phrase))) {
    throw new Error(`${name} failed\nexit=${result.code}\n${result.output}`);
  }
  passed += 1;
  console.log(`  passed ${name}`);
}

try {
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', () => {
      port = server.address().port;
      resolve();
    });
  });

  await check(
    'production live verification requires keyserver redemption evidence',
    'positive',
    1,
    'OSL_KEYSERVER_REDEMPTION_EVIDENCE',
    { evidence: '' },
  );
  await check(
    'preview live verification does not require keyserver redemption evidence',
    'preview',
    0,
    '3 served files and 5 artifact leaves are bound',
    { evidence: '', verifierBranch: 'preview-branch', verifierEnvironment: 'preview' },
  );
  await check(
    'preview live verification does not require production success.html pro-expiry-limitation evidence',
    'preview-missing-pro-expiry-limitation',
    0,
    '3 served files and 5 artifact leaves are bound',
    { evidence: '', verifierBranch: 'preview-branch', verifierEnvironment: 'preview' },
  );
  await check(H25_ACCEPTANCE_COMMAND, 'positive', 0, '3 served files and 5 artifact leaves are bound');
  await check(
    'missing success.html pro-expiry-limitation refusal',
    'missing-success',
    1,
    'success.html pro-expiry-limitation evidence',
  );
  await check(
    'mutated success.html pro-expiry-limitation refusal',
    'missing-pro-expiry-limitation',
    1,
    'pro-expiry-limitation paragraph',
  );
  await check(
    'misstated success.html pro-expiry-limitation refusal',
    'misstated-pro-expiry-limitation',
    1,
    'missing required limitation copy',
  );
  console.log(`test-live-build: ${passed} cases, 0 failed.`);
} finally {
  await new Promise((resolve) => server.close(resolve));
}
