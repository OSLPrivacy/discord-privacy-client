import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign as cryptoSign } from 'node:crypto';
import { buildServer } from '../src/server.js';
import { canonicalManifestBytes } from '../src/canonical.js';

function newServer(opts = {}) {
  return buildServer({ logger: false, dbFile: ':memory:', ...opts });
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

function freshSigner() {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519');
  const der = publicKey.export({ format: 'der', type: 'spki' });
  const raw = der.subarray(der.length - 32);
  return { privateKey, publicKey, pubB64: raw.toString('base64') };
}

function signedManifestEnvelope(signer, manifest) {
  const bytes = canonicalManifestBytes(manifest);
  const sig = cryptoSign(null, bytes, signer.privateKey);
  return {
    version: 1,
    manifest_b64: bytes.toString('base64'),
    signature_b64: sig.toString('base64'),
    signing_key_b64: signer.pubB64,
  };
}

test('selector-manifest: 503 when not configured', async () => {
  const s = newServer();
  const r = await inject(s, { method: 'GET', url: '/v1/selector-manifest' });
  assert.equal(r.statusCode, 503);
  await s.close();
});

test('selector-manifest: returns envelope verbatim when configured', async () => {
  const signer = freshSigner();
  const manifest = {
    version: 1,
    issued_at_unix_seconds: 1_700_000_000,
    client_min_version: '0.1.0',
    selectors: { MessageContent: 'abcd', MessageTextarea: 'wxyz' },
  };
  const envelope = signedManifestEnvelope(signer, manifest);

  const s = newServer({ selectorManifest: envelope });
  const r = await inject(s, { method: 'GET', url: '/v1/selector-manifest' });
  assert.equal(r.statusCode, 200);
  assert.equal(r.body.version, 1);
  assert.equal(r.body.signing_key_b64, signer.pubB64);
  assert.equal(r.body.manifest_b64, envelope.manifest_b64);
  assert.equal(r.body.signature_b64, envelope.signature_b64);
  await s.close();
});

test('canonicalManifestBytes is stable under selector insertion order', () => {
  const a = {
    version: 1,
    issued_at_unix_seconds: 0,
    client_min_version: '0.0.1',
    selectors: { z: '1', a: '2' },
  };
  const b = {
    version: 1,
    issued_at_unix_seconds: 0,
    client_min_version: '0.0.1',
    selectors: { a: '2', z: '1' },
  };
  assert.deepEqual(canonicalManifestBytes(a), canonicalManifestBytes(b));
});

test('canonicalManifestBytes layout matches spec (LP domain, u32 ver, u64 issued, ...)', () => {
  const m = {
    version: 1,
    issued_at_unix_seconds: 0x10203040,
    client_min_version: '0.1.0',
    selectors: { a: 'b' },
  };
  const got = canonicalManifestBytes(m);

  // Reconstruct expected bytes manually.
  const domain = Buffer.from('discord-privacy-client/selector-manifest/v1', 'utf8');
  const want = [];
  const lp = (s) => {
    const buf = Buffer.from(s, 'utf8');
    const len = Buffer.alloc(4);
    len.writeUInt32BE(buf.length);
    want.push(len);
    want.push(buf);
  };
  lp(domain.toString('utf8'));
  const v = Buffer.alloc(4);
  v.writeUInt32BE(1);
  want.push(v);
  const issued = Buffer.alloc(8);
  issued.writeBigUInt64BE(BigInt(0x10203040));
  want.push(issued);
  lp('0.1.0');
  const cnt = Buffer.alloc(4);
  cnt.writeUInt32BE(1);
  want.push(cnt);
  lp('a');
  lp('b');
  assert.deepEqual(got, Buffer.concat(want));
});
