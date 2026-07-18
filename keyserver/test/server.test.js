import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildServer } from '../src/server.js';

function ts(offsetMs = 0) {
  return new Date(Date.now() + offsetMs).toISOString();
}

function b64(text) {
  return Buffer.from(text, 'utf8').toString('base64');
}

async function newServer() {
  return await buildServer({ logger: false, dbFile: ':memory:' });
}

async function inject(server, opts) {
  const res = await server.inject(opts);
  let body = res.body;
  try {
    body = JSON.parse(res.body);
  } catch {
    /* leave as string */
  }
  return { statusCode: res.statusCode, body };
}

const VALID_REGISTRATION = {
  user_id: 'user-1',
  ik_x25519_pub: b64('x25519-pub'),
  ik_ed25519_pub: b64('ed25519-pub'),
  ik_mlkem768_pub: b64('mlkem768-pub'),
  registration_sig: b64('selfsig'),
};

const validWrappedKey = (overrides = {}) => ({
  content_id: 'msg-1',
  content_type: 'text',
  sender_id: 'user-1',
  recipient_id: 'user-2',
  session_version: 1,
  share_index: 0,
  wrapped_share_blob: b64('wrapped-blob'),
  blob_version: 1,
  single_use: false,
  expires_at: ts(60 * 60 * 1000), // 1 hour from now
  ...overrides,
});

// ---- /v1/healthz ----

test('healthz returns ok', async () => {
  const s = await newServer();
  const r = await inject(s, { method: 'GET', url: '/v1/healthz' });
  assert.equal(r.statusCode, 200);
  assert.deepEqual(r.body, { ok: true });
  await s.close();
});

// ---- /v1/register ----

test('register: initial registration returns 201 with timestamp', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  assert.equal(r.statusCode, 201);
  assert.equal(r.body.user_id, 'user-1');
  assert.match(
    r.body.registered_at,
    /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/,
  );
  await s.close();
});

test('register: re-registration returns 200 with key_rotation_recorded', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, ik_x25519_pub: b64('rotated-pub') },
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.user_id, 'user-1');
  assert.equal(r.body.key_rotation_recorded, true);
  assert.match(r.body.last_rotated_at, /^\d{4}/);
  await s.close();
});

test('register: missing required field returns 400', async () => {
  const s = await newServer();
  for (const f of [
    'user_id',
    'ik_x25519_pub',
    'ik_ed25519_pub',
    'ik_mlkem768_pub',
    'registration_sig',
  ]) {
    const payload = { ...VALID_REGISTRATION };
    delete payload[f];
    const r = await inject(s, {
      method: 'POST',
      url: '/v1/register',
      payload,
    });
    assert.equal(r.statusCode, 400, `missing ${f}`);
    assert.match(r.body.error, new RegExp(f));
  }
  await s.close();
});

test('register: non-base64 key field returns 400', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, ik_x25519_pub: '!!!not-base64!!!' },
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

// ---- /v1/pubkeys/:user_id ----

test('pubkeys: returns registered keys', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  const r = await inject(s, { method: 'GET', url: '/v1/pubkeys/user-1' });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.user_id, 'user-1');
  assert.equal(r.body.ik_x25519_pub, VALID_REGISTRATION.ik_x25519_pub);
  assert.equal(r.body.ik_mlkem768_pub, VALID_REGISTRATION.ik_mlkem768_pub);
  assert.equal(r.body.last_rotated_at, null);
  await s.close();
});

test('pubkeys: returns last_rotated_at after re-registration', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, ik_x25519_pub: b64('rotated') },
  });
  const r = await inject(s, { method: 'GET', url: '/v1/pubkeys/user-1' });
  assert.equal(r.statusCode, 200);
  assert.notEqual(r.body.last_rotated_at, null);
  assert.equal(r.body.ik_x25519_pub, b64('rotated'));
  await s.close();
});

test('pubkeys: 404 on unknown user', async () => {
  const s = await newServer();
  const r = await inject(s, { method: 'GET', url: '/v1/pubkeys/nobody' });
  assert.equal(r.statusCode, 404);
  await s.close();
});

// ---- /v1/wrapped-keys POST ----

test('wrapped-keys POST: 201 on valid upload', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
  });
  assert.equal(r.statusCode, 201);
  assert.equal(r.body.content_id, 'msg-1');
  await s.close();
});

test('wrapped-keys POST: 409 on duplicate content_id', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
  });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
  });
  assert.equal(r.statusCode, 409);
  await s.close();
});

test('wrapped-keys POST: 400 on bad content_type', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ content_type: 'video' }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('wrapped-keys POST: 400 on system message without allow-listed kind', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({
      content_type: 'system',
      system_message_kind: 'unknown-future-kind',
    }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('wrapped-keys POST: accepts allow-listed system message', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({
      content_type: 'system',
      system_message_kind: 'burn-alert',
    }),
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('wrapped-keys POST: rejects non-system kind on non-system message', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({
      content_type: 'text',
      system_message_kind: 'burn-alert',
    }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('wrapped-keys POST: requires display_duration_seconds when single_use', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ single_use: true }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('wrapped-keys POST: rejects display_duration_seconds when not single_use', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ single_use: false, display_duration_seconds: 5 }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

// ---- /v1/wrapped-keys GET ----

test('wrapped-keys GET: returns the row', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
  });
  const r = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/msg-1',
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.content_id, 'msg-1');
  assert.equal(r.body.wrapped_share_blob, b64('wrapped-blob'));
  assert.equal(r.body.single_use, false);
  await s.close();
});

test('wrapped-keys GET: 404 on unknown content_id', async () => {
  const s = await newServer();
  const r = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/nope',
  });
  assert.equal(r.statusCode, 404);
  await s.close();
});

test('wrapped-keys GET: single_use is consumed atomically', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ single_use: true, display_duration_seconds: 30 }),
  });
  const r1 = await inject(s, { method: 'GET', url: '/v1/wrapped-keys/msg-1' });
  assert.equal(r1.statusCode, 200);
  assert.equal(r1.body.single_use, true);
  const r2 = await inject(s, { method: 'GET', url: '/v1/wrapped-keys/msg-1' });
  assert.equal(r2.statusCode, 404);
  await s.close();
});

test('wrapped-keys GET: 410 on past-expiry tombstone', async () => {
  const s = await newServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ expires_at: ts(-1000) }),
  });
  const r = await inject(s, { method: 'GET', url: '/v1/wrapped-keys/msg-1' });
  assert.equal(r.statusCode, 410);
  // Subsequent fetch returns 404 (lazy-tombstoned).
  const r2 = await inject(s, { method: 'GET', url: '/v1/wrapped-keys/msg-1' });
  assert.equal(r2.statusCode, 404);
  await s.close();
});

// ---- end-to-end through the full set ----

test('end-to-end: register, fetch pubkeys, upload, fetch wrapped key', async () => {
  const s = await newServer();
  // Alice registers.
  const reg = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  assert.equal(reg.statusCode, 201);
  // Bob looks her up.
  const lookup = await inject(s, {
    method: 'GET',
    url: '/v1/pubkeys/user-1',
  });
  assert.equal(lookup.statusCode, 200);
  // Alice uploads a wrapped blob for Bob.
  const upload = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ recipient_id: 'user-2' }),
  });
  assert.equal(upload.statusCode, 201);
  // Bob fetches it.
  const fetched = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/msg-1',
  });
  assert.equal(fetched.statusCode, 200);
  assert.equal(fetched.body.recipient_id, 'user-2');
  await s.close();
});

// ============================================================
// Phase B: admin token + allowlist auth
// ============================================================

const ADMIN_TOKEN = 'test-token-deadbeefcafebabe1234567890abcdef';
const WRONG_TOKEN = 'test-token-WRONG-WRONG-WRONG-WRONG-WRONG-WR';

async function authedServer({ adminToken = ADMIN_TOKEN, allowedUsers = null } = {}) {
  return await buildServer({
    logger: false,
    dbFile: ':memory:',
    adminToken,
    allowedUsers,
  });
}

const bearer = (token) => ({ authorization: `Bearer ${token}` });

// ---- token check on /v1/register ----

test('admin auth: register without token returns 401 when token configured', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  assert.equal(r.statusCode, 401);
  assert.equal(r.body.error, 'unauthorized');
  await s.close();
});

test('admin auth: register with wrong token returns 401', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
    headers: bearer(WRONG_TOKEN),
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

test('admin auth: register with malformed Authorization header returns 401', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
    headers: { authorization: 'Basic abc123' },
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

test('admin auth: register with correct token returns 201', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
    headers: bearer(ADMIN_TOKEN),
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('admin auth: case-insensitive Bearer prefix accepted', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
    headers: { authorization: `bearer ${ADMIN_TOKEN}` },
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('admin auth: dev mode (no token configured) accepts unauthed register', async () => {
  const s = await newServer(); // no adminToken option
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('admin auth: empty-string token treated as dev mode', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    adminToken: '',
  });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

// ---- allowlist on /v1/register ----

test('allowlist: register with disallowed user_id returns 403 (token was valid)', async () => {
  const s = await authedServer({ allowedUsers: ['liam', 'henry'] });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, user_id: 'mallory' },
    headers: bearer(ADMIN_TOKEN),
  });
  assert.equal(r.statusCode, 403);
  assert.match(r.body.error, /allowlist/);
  await s.close();
});

test('allowlist: register with allowed user_id returns 201', async () => {
  const s = await authedServer({ allowedUsers: ['liam', 'henry'] });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, user_id: 'liam' },
    headers: bearer(ADMIN_TOKEN),
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('allowlist: empty array treated as disabled (any user_id allowed)', async () => {
  const s = await authedServer({ allowedUsers: [] });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
    headers: bearer(ADMIN_TOKEN),
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('allowlist: token check runs before allowlist (no header → 401, not 403)', async () => {
  const s = await authedServer({ allowedUsers: ['liam'] });
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, user_id: 'mallory' },
    // no Authorization header
  });
  assert.equal(r.statusCode, 401, 'token check should gate first');
  await s.close();
});

// ---- token check on other mutation routes ----

test('admin auth: POST /v1/wrapped-keys without token returns 401', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

test('admin auth: POST /v1/wrapped-keys with token returns 201', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
    headers: bearer(ADMIN_TOKEN),
  });
  assert.equal(r.statusCode, 201);
  await s.close();
});

test('admin auth: DELETE /v1/wrapped-keys without token returns 401', async () => {
  const s = await authedServer();
  // Body is rejected at auth before sig validation runs.
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'all',
      user_id: 'user-1',
      burn_signature_b64: b64('does-not-matter'),
    },
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

test('admin auth: POST /v1/prekey-bundle/replenish without token returns 401', async () => {
  const s = await authedServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: b64('does-not-matter'),
      opks: [],
    },
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

// ---- GET endpoints stay public regardless of token ----

test('admin auth: GET /v1/healthz works without token in authed mode', async () => {
  const s = await authedServer();
  const r = await inject(s, { method: 'GET', url: '/v1/healthz' });
  assert.equal(r.statusCode, 200);
  await s.close();
});

test('admin auth: GET /v1/pubkeys works without token in authed mode', async () => {
  const s = await authedServer();
  // Register first (with token).
  await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: VALID_REGISTRATION,
    headers: bearer(ADMIN_TOKEN),
  });
  // Lookup with no header.
  const r = await inject(s, { method: 'GET', url: '/v1/pubkeys/user-1' });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.user_id, 'user-1');
  await s.close();
});

test('admin auth: GET /v1/wrapped-keys works without token in authed mode', async () => {
  const s = await authedServer();
  await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey(),
    headers: bearer(ADMIN_TOKEN),
  });
  const r = await inject(s, { method: 'GET', url: '/v1/wrapped-keys/msg-1' });
  assert.equal(r.statusCode, 200);
  await s.close();
});

test('admin auth: GET /v1/prekey-bundle works without token in authed mode', async () => {
  const s = await authedServer();
  // Empty pool returns 404 even with auth working — confirms the
  // route ran past the auth layer.
  const r = await inject(s, { method: 'GET', url: '/v1/prekey-bundle/nobody' });
  assert.equal(r.statusCode, 404);
  await s.close();
});

// ---- rate limit: opt-in via buildServer config ----

test('rate limit: 11th mutation request within window returns 429', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    adminToken: ADMIN_TOKEN,
    rateLimit: { max: 3, timeWindow: '1 minute' },
  });
  // Three OK, fourth limited.
  for (let i = 0; i < 3; i++) {
    const r = await inject(s, {
      method: 'POST',
      url: '/v1/register',
      payload: { ...VALID_REGISTRATION, user_id: `u-${i}` },
      headers: bearer(ADMIN_TOKEN),
    });
    assert.ok(r.statusCode === 201 || r.statusCode === 200, `req ${i}: ${r.statusCode}`);
  }
  const limited = await inject(s, {
    method: 'POST',
    url: '/v1/register',
    payload: { ...VALID_REGISTRATION, user_id: 'u-4' },
    headers: bearer(ADMIN_TOKEN),
  });
  assert.equal(limited.statusCode, 429);
  await s.close();
});

test('rate limit: GET endpoints not rate-limited', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    adminToken: ADMIN_TOKEN,
    rateLimit: { max: 3, timeWindow: '1 minute' },
  });
  // Hammer healthz past the limit; all should succeed.
  for (let i = 0; i < 10; i++) {
    const r = await inject(s, { method: 'GET', url: '/v1/healthz' });
    assert.equal(r.statusCode, 200);
  }
  await s.close();
});

// ----- F1.4 cutover: redirectTarget mode -----

test('F1.4 redirect: /v1/healthz returns 200 (no redirect — keeps Railway healthcheck green)', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    redirectTarget: 'https://keyserver.oslprivacy.com',
  });
  const r = await inject(s, { method: 'GET', url: '/v1/healthz' });
  assert.equal(r.statusCode, 200);
  await s.close();
});

test('F1.4 redirect: all non-healthz routes 308 with Location preserving path', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    redirectTarget: 'https://keyserver.oslprivacy.com',
  });
  // Use raw .inject so we get headers (the local `inject` helper
  // strips them).
  const r = await s.inject({ method: 'GET', url: '/v1/pubkeys/alice' });
  assert.equal(r.statusCode, 308);
  assert.equal(
    r.headers.location,
    'https://keyserver.oslprivacy.com/v1/pubkeys/alice',
  );
  const r2 = await s.inject({
    method: 'POST',
    url: '/v1/register',
    payload: { user_id: 'x' },
  });
  assert.equal(r2.statusCode, 308);
  assert.equal(
    r2.headers.location,
    'https://keyserver.oslprivacy.com/v1/register',
  );
  await s.close();
});

test('F1.4 redirect: trailing slash on redirectTarget is normalised', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    redirectTarget: 'https://keyserver.oslprivacy.com///',
  });
  const r = await s.inject({ method: 'GET', url: '/v1/pubkeys/x' });
  assert.equal(r.statusCode, 308);
  assert.equal(
    r.headers.location,
    'https://keyserver.oslprivacy.com/v1/pubkeys/x',
  );
  await s.close();
});

test('F1.4 redirect: unset redirectTarget is a no-op (default-serving mode)', async () => {
  const s = await buildServer({
    logger: false,
    dbFile: ':memory:',
    // redirectTarget omitted → falls through normal routing.
  });
  // /v1/pubkeys for an unknown user_id is the 404 path; we're
  // confirming we hit that path, not a 308.
  const r = await inject(s, { method: 'GET', url: '/v1/pubkeys/alice' });
  assert.equal(r.statusCode, 404);
  await s.close();
});
