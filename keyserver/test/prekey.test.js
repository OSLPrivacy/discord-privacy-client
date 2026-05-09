import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  generateKeyPairSync,
  sign as cryptoSign,
  createPublicKey,
} from 'node:crypto';
import { buildServer } from '../src/server.js';
import { canonicalReplenishBytes } from '../src/canonical.js';

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

// Helper: generate an Ed25519 keypair and return raw 32-byte pub +
// secret as a SigningKey, plus a base64-encoded raw pub for the
// register request.
function makeIdentity(userId) {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const rawPub = publicKey.export({ format: 'der', type: 'spki' });
  // SPKI Ed25519 DER prefix is 12 bytes; the last 32 bytes are the
  // raw key.
  const rawPubBytes = rawPub.subarray(rawPub.length - 32);
  return {
    user_id: userId,
    privateKey,
    publicKey,
    ik_ed25519_pub_raw: rawPubBytes,
    ik_ed25519_pub_b64: Buffer.from(rawPubBytes).toString('base64'),
  };
}

function signBuffer(privateKey, message) {
  return cryptoSign(null, message, privateKey);
}

async function registerIdentity(server, identity, otherFields = {}) {
  return inject(server, {
    method: 'POST',
    url: '/v1/register',
    payload: {
      user_id: identity.user_id,
      ik_x25519_pub: 'AAAA',
      ik_ed25519_pub: identity.ik_ed25519_pub_b64,
      ik_mlkem768_pub: 'BBBB',
      ik_x25519_signature: 'Q0M=',
      ...otherFields,
    },
  });
}

function makeOpks(count, startId = 0) {
  const opks = [];
  for (let i = 0; i < count; i++) {
    opks.push({
      id: startId + i,
      pub_b64: Buffer.from([i, 0, 0]).toString('base64'),
    });
  }
  return opks;
}

function makeSpk(rotatedAt = '2026-05-08T12:00:00Z') {
  return {
    pub_b64: Buffer.from('spk-public').toString('base64'),
    signature_b64: Buffer.from('spk-signature').toString('base64'),
    rotated_at: rotatedAt,
  };
}

test('prekey-bundle GET: 404 before any replenish', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const r = await inject(s, {
    method: 'GET',
    url: '/v1/prekey-bundle/user-1',
  });
  assert.equal(r.statusCode, 404);
  await s.close();
});

test('prekey-bundle replenish: 401 when batch signature wrong', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const opks = makeOpks(5);
  const spk = makeSpk();
  // Sign with a *different* key.
  const other = makeIdentity('other');
  const message = canonicalReplenishBytes({
    user_id: 'user-1',
    spk,
    opks,
  });
  const wrongSig = signBuffer(other.privateKey, message);
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: wrongSig.toString('base64'),
      spk,
      opks,
    },
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

test('prekey-bundle replenish: 401 when message tampered', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const opks = makeOpks(3);
  const spk = makeSpk();
  // Sign the canonical bytes for ONE OPK list, then send a different
  // OPK list. The signature over the original bytes won't verify
  // against the (different) canonical bytes the server reconstructs.
  const wrongOpks = makeOpks(3, 100);
  const messageOriginal = canonicalReplenishBytes({
    user_id: 'user-1',
    spk,
    opks,
  });
  const sig = signBuffer(id.privateKey, messageOriginal);
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: sig.toString('base64'),
      spk,
      opks: wrongOpks,
    },
  });
  assert.equal(r.statusCode, 401);
  await s.close();
});

test('prekey-bundle replenish: 200 with valid signature, GET returns bundle', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const opks = makeOpks(5);
  const spk = makeSpk();
  const message = canonicalReplenishBytes({
    user_id: 'user-1',
    spk,
    opks,
  });
  const sig = signBuffer(id.privateKey, message);
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: sig.toString('base64'),
      spk,
      opks,
    },
  });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.opks_added, 5);

  // Now GET the bundle — should pop one OPK.
  const g = await inject(s, {
    method: 'GET',
    url: '/v1/prekey-bundle/user-1',
  });
  assert.equal(g.statusCode, 200);
  assert.equal(g.body.user_id, 'user-1');
  assert.equal(g.body.spk_pub, spk.pub_b64);
  assert.notEqual(g.body.opk, null);
  assert.equal(g.body.remaining_opk_count, 4);
  await s.close();
});

test('prekey-bundle GET: pops OPKs atomically until exhausted, then null', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const opks = makeOpks(3);
  const spk = makeSpk();
  const message = canonicalReplenishBytes({
    user_id: 'user-1',
    spk,
    opks,
  });
  const sig = signBuffer(id.privateKey, message);
  await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: sig.toString('base64'),
      spk,
      opks,
    },
  });
  const seen = new Set();
  for (let i = 0; i < 3; i++) {
    const g = await inject(s, {
      method: 'GET',
      url: '/v1/prekey-bundle/user-1',
    });
    assert.equal(g.statusCode, 200);
    assert.notEqual(g.body.opk, null);
    seen.add(g.body.opk.id);
    assert.equal(g.body.remaining_opk_count, 2 - i);
  }
  assert.equal(seen.size, 3, 'each fetch returned a distinct OPK');
  // Pool exhausted — opk: null per the design fallback.
  const g = await inject(s, {
    method: 'GET',
    url: '/v1/prekey-bundle/user-1',
  });
  assert.equal(g.statusCode, 200);
  assert.equal(g.body.opk, null);
  await s.close();
});

test('prekey-bundle replenish: SPK rotation moves current to prev', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const spkA = makeSpk('2026-05-01T00:00:00Z');
  const messageA = canonicalReplenishBytes({
    user_id: 'user-1',
    spk: spkA,
    opks: [],
  });
  await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: signBuffer(id.privateKey, messageA).toString(
        'base64',
      ),
      spk: spkA,
      opks: [],
    },
  });
  // Need at least one OPK to fetch the bundle, so add some.
  const opks = makeOpks(1);
  const messageOpks = canonicalReplenishBytes({
    user_id: 'user-1',
    spk: null,
    opks,
  });
  await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: signBuffer(id.privateKey, messageOpks).toString(
        'base64',
      ),
      opks,
    },
  });
  const g1 = await inject(s, {
    method: 'GET',
    url: '/v1/prekey-bundle/user-1',
  });
  assert.equal(g1.body.spk_pub, spkA.pub_b64);

  // Now rotate to spkB.
  const spkB = makeSpk('2026-05-08T00:00:00Z');
  const messageB = canonicalReplenishBytes({
    user_id: 'user-1',
    spk: spkB,
    opks: [],
  });
  await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: signBuffer(id.privateKey, messageB).toString(
        'base64',
      ),
      spk: spkB,
      opks: [],
    },
  });
  // Add another OPK so GET works.
  const opks2 = makeOpks(1, 100);
  const messageOpks2 = canonicalReplenishBytes({
    user_id: 'user-1',
    spk: null,
    opks: opks2,
  });
  await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: signBuffer(id.privateKey, messageOpks2).toString(
        'base64',
      ),
      opks: opks2,
    },
  });
  const g2 = await inject(s, {
    method: 'GET',
    url: '/v1/prekey-bundle/user-1',
  });
  assert.equal(g2.body.spk_pub, spkB.pub_b64);
  await s.close();
});

test('prekey-bundle replenish: 404 before register', async () => {
  const s = newServer();
  const id = makeIdentity('nope');
  const opks = makeOpks(1);
  const message = canonicalReplenishBytes({
    user_id: 'nope',
    spk: null,
    opks,
  });
  const sig = signBuffer(id.privateKey, message);
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'nope',
      batch_signature_b64: sig.toString('base64'),
      opks,
    },
  });
  assert.equal(r.statusCode, 404);
  await s.close();
});

test('prekey-bundle replenish: 400 on missing fields', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  for (const drop of ['user_id', 'batch_signature_b64', 'opks']) {
    const payload = {
      user_id: 'user-1',
      batch_signature_b64: 'AA',
      opks: [],
    };
    delete payload[drop];
    const r = await inject(s, {
      method: 'POST',
      url: '/v1/prekey-bundle/replenish',
      payload,
    });
    assert.equal(r.statusCode, 400, `missing ${drop}`);
  }
  await s.close();
});

test('prekey-bundle replenish: 409 on duplicate opk id', async () => {
  const s = newServer();
  const id = makeIdentity('user-1');
  await registerIdentity(s, id);
  const opks = makeOpks(2);
  const message = canonicalReplenishBytes({
    user_id: 'user-1',
    spk: makeSpk(),
    opks,
  });
  const sig = signBuffer(id.privateKey, message);
  await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: sig.toString('base64'),
      spk: makeSpk(),
      opks,
    },
  });
  // Submit again with an overlapping id.
  const opks2 = [opks[0]]; // same id 0
  const message2 = canonicalReplenishBytes({
    user_id: 'user-1',
    spk: null,
    opks: opks2,
  });
  const sig2 = signBuffer(id.privateKey, message2);
  const r = await inject(s, {
    method: 'POST',
    url: '/v1/prekey-bundle/replenish',
    payload: {
      user_id: 'user-1',
      batch_signature_b64: sig2.toString('base64'),
      opks: opks2,
    },
  });
  assert.equal(r.statusCode, 409);
  await s.close();
});

test('canonical.verifyEd25519 rejects mismatched key sizes', async () => {
  const { verifyEd25519 } = await import('../src/canonical.js');
  // Wrong-size public key.
  const ok = verifyEd25519(Buffer.alloc(16), Buffer.from('m'), Buffer.alloc(64));
  assert.equal(ok, false);
  // Wrong-size signature.
  const ok2 = verifyEd25519(
    Buffer.alloc(32),
    Buffer.from('m'),
    Buffer.alloc(48),
  );
  assert.equal(ok2, false);
});
