import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { buildServer } from '../src/server.js';
import { canonicalBurnBytes } from '../src/canonical.js';

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

function makeIdentity(userId) {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const rawPub = publicKey.export({ format: 'der', type: 'spki' });
  const rawPubBytes = rawPub.subarray(rawPub.length - 32);
  return {
    user_id: userId,
    privateKey,
    publicKey,
    ik_ed25519_pub_b64: Buffer.from(rawPubBytes).toString('base64'),
  };
}

async function registerIdentity(server, identity) {
  return inject(server, {
    method: 'POST',
    url: '/v1/register',
    payload: {
      user_id: identity.user_id,
      ik_x25519_pub: 'AAAA',
      ik_ed25519_pub: identity.ik_ed25519_pub_b64,
      ik_mlkem768_pub: 'BBBB',
      ik_x25519_signature: 'Q0M=',
    },
  });
}

function uploadWrappedKey(server, contentId, senderId, recipientId) {
  return inject(server, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: {
      content_id: contentId,
      content_type: 'text',
      sender_id: senderId,
      recipient_id: recipientId,
      session_version: 1,
      share_index: 0,
      wrapped_share_blob: Buffer.from(`blob-${contentId}`).toString('base64'),
      blob_version: 1,
      single_use: false,
      expires_at: new Date(Date.now() + 3_600_000).toISOString(),
    },
  });
}

function signBurn(privateKey, payload) {
  const message = canonicalBurnBytes(payload);
  return cryptoSign(null, message, privateKey).toString('base64');
}

test('burn single: deletes own content, leaves others', async () => {
  const s = newServer();
  const alice = makeIdentity('alice');
  const bob = makeIdentity('bob');
  await registerIdentity(s, alice);
  await registerIdentity(s, bob);

  await uploadWrappedKey(s, 'msg-alice-1', 'alice', 'bob');
  await uploadWrappedKey(s, 'msg-alice-2', 'alice', 'bob');
  await uploadWrappedKey(s, 'msg-bob-1', 'bob', 'alice');

  const sig = signBurn(alice.privateKey, {
    user_id: 'alice',
    scope: 'single',
    target: { content_id: 'msg-alice-1' },
  });

  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'single',
      user_id: 'alice',
      target_content_id: 'msg-alice-1',
      burn_signature_b64: sig,
    },
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.deleted_count, 1);

  // msg-alice-1 gone, msg-alice-2 still here, msg-bob-1 untouched.
  const g1 = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/msg-alice-1',
  });
  assert.equal(g1.statusCode, 404);
  const g2 = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/msg-alice-2',
  });
  assert.equal(g2.statusCode, 200);
  const g3 = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/msg-bob-1',
  });
  assert.equal(g3.statusCode, 200);
  await s.close();
});

test('burn single: cannot burn another user\'s content (signature is over alice but content is bob\'s)', async () => {
  const s = newServer();
  const alice = makeIdentity('alice');
  const bob = makeIdentity('bob');
  await registerIdentity(s, alice);
  await registerIdentity(s, bob);
  await uploadWrappedKey(s, 'bob-msg', 'bob', 'alice');

  // Alice signs a burn of bob's content. Server validates the
  // signature (passes) but the DELETE statement filters
  // sender_id = 'alice' so bob's row is untouched. deleted_count = 0.
  const sig = signBurn(alice.privateKey, {
    user_id: 'alice',
    scope: 'single',
    target: { content_id: 'bob-msg' },
  });
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'single',
      user_id: 'alice',
      target_content_id: 'bob-msg',
      burn_signature_b64: sig,
    },
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.deleted_count, 0);
  // bob's content still present.
  const g = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/bob-msg',
  });
  assert.equal(g.statusCode, 200);
  await s.close();
});

test('burn to_user: deletes only messages between sender and target', async () => {
  const s = newServer();
  const alice = makeIdentity('alice');
  const bob = makeIdentity('bob');
  const carol = makeIdentity('carol');
  await registerIdentity(s, alice);
  await registerIdentity(s, bob);
  await registerIdentity(s, carol);

  await uploadWrappedKey(s, 'a-to-b-1', 'alice', 'bob');
  await uploadWrappedKey(s, 'a-to-b-2', 'alice', 'bob');
  await uploadWrappedKey(s, 'a-to-c-1', 'alice', 'carol');
  await uploadWrappedKey(s, 'b-to-a-1', 'bob', 'alice');

  const sig = signBurn(alice.privateKey, {
    user_id: 'alice',
    scope: 'to_user',
    target: { user_id: 'bob' },
  });
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'to_user',
      user_id: 'alice',
      target_user_id: 'bob',
      burn_signature_b64: sig,
    },
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.deleted_count, 2);

  assert.equal(
    (await inject(s, { method: 'GET', url: '/v1/wrapped-keys/a-to-b-1' })).statusCode,
    404,
  );
  assert.equal(
    (await inject(s, { method: 'GET', url: '/v1/wrapped-keys/a-to-b-2' })).statusCode,
    404,
  );
  // alice→carol not touched.
  assert.equal(
    (await inject(s, { method: 'GET', url: '/v1/wrapped-keys/a-to-c-1' })).statusCode,
    200,
  );
  // bob→alice not touched (alice is recipient, not sender).
  assert.equal(
    (await inject(s, { method: 'GET', url: '/v1/wrapped-keys/b-to-a-1' })).statusCode,
    200,
  );
  await s.close();
});

test('burn all: deletes every message alice sent', async () => {
  const s = newServer();
  const alice = makeIdentity('alice');
  const bob = makeIdentity('bob');
  await registerIdentity(s, alice);
  await registerIdentity(s, bob);

  await uploadWrappedKey(s, 'a1', 'alice', 'bob');
  await uploadWrappedKey(s, 'a2', 'alice', 'bob');
  await uploadWrappedKey(s, 'a3', 'alice', 'alice');
  await uploadWrappedKey(s, 'b1', 'bob', 'alice');

  const sig = signBurn(alice.privateKey, {
    user_id: 'alice',
    scope: 'all',
    target: undefined,
  });
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'all',
      user_id: 'alice',
      burn_signature_b64: sig,
    },
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.deleted_count, 3);

  for (const id of ['a1', 'a2', 'a3']) {
    assert.equal(
      (await inject(s, { method: 'GET', url: `/v1/wrapped-keys/${id}` })).statusCode,
      404,
    );
  }
  // bob→alice still here.
  assert.equal(
    (await inject(s, { method: 'GET', url: '/v1/wrapped-keys/b1' })).statusCode,
    200,
  );
  await s.close();
});

test('burn: 401 on bad signature', async () => {
  const s = newServer();
  const alice = makeIdentity('alice');
  const evil = makeIdentity('evil-other-key');
  await registerIdentity(s, alice);
  await uploadWrappedKey(s, 'a1', 'alice', 'bob');

  const sig = signBurn(evil.privateKey, {
    user_id: 'alice',
    scope: 'single',
    target: { content_id: 'a1' },
  });
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'single',
      user_id: 'alice',
      target_content_id: 'a1',
      burn_signature_b64: sig,
    },
  });
  assert.equal(r.statusCode, 401);
  // Content untouched.
  const g = await inject(s, { method: 'GET', url: '/v1/wrapped-keys/a1' });
  assert.equal(g.statusCode, 200);
  await s.close();
});

test('burn: 400 on missing fields', async () => {
  const s = newServer();
  for (const drop of ['scope', 'user_id', 'burn_signature_b64']) {
    const payload = {
      scope: 'all',
      user_id: 'alice',
      burn_signature_b64: 'AA',
    };
    delete payload[drop];
    const r = await inject(s, {
      method: 'DELETE',
      url: '/v1/wrapped-keys',
      payload,
    });
    assert.equal(r.statusCode, 400, `missing ${drop}`);
  }
  await s.close();
});

test('burn single: 400 when target_content_id missing', async () => {
  const s = newServer();
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'single',
      user_id: 'alice',
      burn_signature_b64: 'AA',
    },
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('burn to_user: 400 when target_user_id missing', async () => {
  const s = newServer();
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'to_user',
      user_id: 'alice',
      burn_signature_b64: 'AA',
    },
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('burn all: 400 when target field present', async () => {
  const s = newServer();
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'all',
      user_id: 'alice',
      target_content_id: 'oops',
      burn_signature_b64: 'AA',
    },
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('burn: 400 on unknown scope', async () => {
  const s = newServer();
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'evil-mode',
      user_id: 'alice',
      burn_signature_b64: 'AA',
    },
  });
  assert.equal(r.statusCode, 400);
  await s.close();
});

test('burn: 404 before register', async () => {
  const s = newServer();
  const id = makeIdentity('nobody');
  const sig = signBurn(id.privateKey, {
    user_id: 'nobody',
    scope: 'all',
    target: undefined,
  });
  const r = await inject(s, {
    method: 'DELETE',
    url: '/v1/wrapped-keys',
    payload: {
      scope: 'all',
      user_id: 'nobody',
      burn_signature_b64: sig,
    },
  });
  assert.equal(r.statusCode, 404);
  await s.close();
});

test('burn-and-alert via wrapped-keys system message: caller signs system blob', async () => {
  // The burn-alert path is the existing POST /v1/wrapped-keys with
  // content_type=system, system_message_kind=burn-alert. This test
  // confirms the existing path still accepts allowed system kinds
  // alongside burns — there's no separate "alert" endpoint.
  const s = newServer();
  const alice = makeIdentity('alice');
  await registerIdentity(s, alice);
  // Upload a burn-alert system message.
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/wrapped-keys',
    payload: {
      content_id: 'burn-alert-1',
      content_type: 'system',
      system_message_kind: 'burn-alert',
      sender_id: 'alice',
      recipient_id: 'bob',
      session_version: 1,
      share_index: 0,
      wrapped_share_blob: Buffer.from('signed alert payload').toString('base64'),
      blob_version: 1,
      single_use: false,
      expires_at: new Date(Date.now() + 3_600_000).toISOString(),
    },
  });
  assert.equal(r.statusCode, 201);

  // Receiver fetches and decodes; the wrapped_share_blob is the
  // signed-and-encrypted text the recipient verifies client-side.
  const g = await inject(s, {
    method: 'GET',
    url: '/v1/wrapped-keys/burn-alert-1',
  });
  assert.equal(g.statusCode, 200);
  assert.equal(g.body.content_type, 'system');
  assert.equal(g.body.system_message_kind, 'burn-alert');
  await s.close();
});
