import { test } from 'node:test';
import assert from 'node:assert/strict';
import { buildServer } from '../src/server.js';

function ts(offsetMs = 0) {
  return new Date(Date.now() + offsetMs).toISOString();
}

function b64(text) {
  return Buffer.from(text, 'utf8').toString('base64');
}

function newServer() {
  return buildServer({ logger: false, dbFile: ':memory:' });
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
  ik_mlkem768_pub: b64('mlkem768-pub'),
  ik_x25519_signature: b64('selfsig'),
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
  const s = newServer();
  const r = await inject(s, { method: 'GET', url: '/v1/healthz' });
  assert.equal(r.statusCode, 200);
  assert.deepEqual(r.body, { ok: true });
  await s.close();
});

// ---- /v1/register ----

test('register: initial registration returns 201 with timestamp', async () => {
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
  for (const f of ['user_id', 'ik_x25519_pub', 'ik_mlkem768_pub', 'ik_x25519_signature']) {
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
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
  const r = await inject(s, { method: 'GET', url: '/v1/pubkeys/nobody' });
  assert.equal(r.statusCode, 404);
  await s.close();
});

// ---- /v1/wrapped-keys POST ----

test('wrapped-keys POST: 201 on valid upload', async () => {
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ content_type: 'video' }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('wrapped-keys POST: 400 on system message without allow-listed kind', async () => {
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: validWrappedKey({ single_use: true }),
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('wrapped-keys POST: rejects display_duration_seconds when not single_use', async () => {
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
  const r = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/nope',
  });
  assert.equal(r.statusCode, 404);
  await s.close();
});

test('wrapped-keys GET: single_use is consumed atomically', async () => {
  const s = newServer();
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
  const s = newServer();
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
  const s = newServer();
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
