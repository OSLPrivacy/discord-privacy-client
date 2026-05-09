// Prototype key server for discord-privacy-client.
//
// INSECURE BY DESIGN — see `db.js` and `keyserver/README.md`. This is
// a v1-alpha-prototype scaffold. v1 stable replaces it with the
// authenticated, TLS-only, OAuth-gated, rate-limited service in
// `docs/design/key-server-api.md`.
//
// Endpoints (subset of the design doc):
//   POST   /v1/register
//   GET    /v1/pubkeys/:user_id
//   POST   /v1/wrapped-keys
//   GET    /v1/wrapped-keys/:content_id
//   GET    /v1/healthz
//
// All other endpoints in the design doc (prekey-bundle, replenish,
// burn, sessions/rotate, tokens/issue) are deferred.

import Fastify from 'fastify';
import {
  openDatabase,
  upsertUser,
  getUser,
  insertWrappedKey,
  fetchWrappedKey,
} from './db.js';

const ALLOWED_CONTENT_TYPES = new Set(['text', 'attachment', 'system']);
const ALLOWED_SYSTEM_KINDS = new Set(['burn-alert']);

// Lightweight base64 sanity check (no padding tolerance bias) — the
// prototype only validates *shape*, not key validity. Cryptographic
// validation lives client-side.
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;

function isNonEmptyBase64(value) {
  return typeof value === 'string' && value.length > 0 && BASE64_RE.test(value);
}

function isPlainString(value) {
  return typeof value === 'string' && value.length > 0;
}

export function buildServer({ logger = false, dbFile = ':memory:' } = {}) {
  const fastify = Fastify({ logger });
  const db = openDatabase(dbFile);

  fastify.addHook('onClose', async () => {
    db.close();
  });

  fastify.get('/v1/healthz', async () => ({ ok: true }));

  // ---- POST /v1/register ----
  fastify.post('/v1/register', async (request, reply) => {
    const body = request.body ?? {};
    const required = [
      'user_id',
      'ik_x25519_pub',
      'ik_mlkem768_pub',
      'ik_x25519_signature',
    ];
    for (const field of required) {
      if (!(field in body)) {
        return reply.code(400).send({ error: `missing field: ${field}` });
      }
    }
    if (!isPlainString(body.user_id)) {
      return reply.code(400).send({ error: 'user_id must be a non-empty string' });
    }
    for (const k of ['ik_x25519_pub', 'ik_mlkem768_pub', 'ik_x25519_signature']) {
      if (!isNonEmptyBase64(body[k])) {
        return reply.code(400).send({ error: `${k} must be base64` });
      }
    }
    const result = upsertUser(db, body);
    if (result.isNew) {
      return reply.code(201).send({
        user_id: body.user_id,
        registered_at: result.registered_at,
      });
    }
    return reply.code(200).send({
      user_id: body.user_id,
      key_rotation_recorded: true,
      last_rotated_at: result.last_rotated_at,
    });
  });

  // ---- GET /v1/pubkeys/:user_id ----
  fastify.get('/v1/pubkeys/:user_id', async (request, reply) => {
    const row = getUser(db, request.params.user_id);
    if (!row) return reply.code(404).send({ error: 'unknown user_id' });
    return reply.send(row);
  });

  // ---- POST /v1/wrapped-keys ----
  fastify.post('/v1/wrapped-keys', async (request, reply) => {
    const b = request.body ?? {};
    const required = [
      'content_id',
      'content_type',
      'sender_id',
      'recipient_id',
      'session_version',
      'share_index',
      'wrapped_share_blob',
      'blob_version',
      'single_use',
      'expires_at',
    ];
    for (const field of required) {
      if (!(field in b)) {
        return reply.code(400).send({ error: `missing field: ${field}` });
      }
    }
    if (!isPlainString(b.content_id)) {
      return reply.code(400).send({ error: 'content_id must be a non-empty string' });
    }
    if (!ALLOWED_CONTENT_TYPES.has(b.content_type)) {
      return reply.code(400).send({
        error: `content_type must be one of ${[...ALLOWED_CONTENT_TYPES].join(', ')}`,
      });
    }
    if (b.content_type === 'system') {
      if (!ALLOWED_SYSTEM_KINDS.has(b.system_message_kind)) {
        return reply.code(400).send({
          error: `system_message_kind must be one of ${[...ALLOWED_SYSTEM_KINDS].join(', ')}`,
        });
      }
    } else if (b.system_message_kind != null) {
      return reply.code(400).send({
        error: 'system_message_kind only valid when content_type=system',
      });
    }
    if (!isPlainString(b.sender_id) || !isPlainString(b.recipient_id)) {
      return reply.code(400).send({
        error: 'sender_id / recipient_id must be non-empty strings',
      });
    }
    if (!Number.isInteger(b.session_version) || b.session_version < 1) {
      return reply.code(400).send({ error: 'session_version must be a positive integer' });
    }
    if (!Number.isInteger(b.share_index) || b.share_index < 0) {
      return reply.code(400).send({
        error: 'share_index must be a non-negative integer',
      });
    }
    if (!isNonEmptyBase64(b.wrapped_share_blob)) {
      return reply.code(400).send({ error: 'wrapped_share_blob must be base64' });
    }
    if (!Number.isInteger(b.blob_version) || b.blob_version < 1) {
      return reply.code(400).send({ error: 'blob_version must be a positive integer' });
    }
    if (typeof b.single_use !== 'boolean') {
      return reply.code(400).send({ error: 'single_use must be a boolean' });
    }
    if (b.single_use && typeof b.display_duration_seconds !== 'number') {
      return reply.code(400).send({
        error: 'display_duration_seconds required when single_use=true',
      });
    }
    if (!b.single_use && b.display_duration_seconds != null) {
      return reply.code(400).send({
        error: 'display_duration_seconds only valid when single_use=true',
      });
    }
    if (Number.isNaN(Date.parse(b.expires_at))) {
      return reply.code(400).send({ error: 'expires_at must be ISO-8601' });
    }

    try {
      insertWrappedKey(db, {
        ...b,
        single_use: b.single_use ? 1 : 0,
        display_duration_seconds: b.display_duration_seconds ?? null,
        system_message_kind: b.system_message_kind ?? null,
      });
    } catch (err) {
      if (err.code === 'SQLITE_CONSTRAINT_PRIMARYKEY') {
        return reply.code(409).send({ error: 'content_id already exists' });
      }
      throw err;
    }
    return reply.code(201).send({ content_id: b.content_id });
  });

  // ---- GET /v1/wrapped-keys/:content_id ----
  fastify.get('/v1/wrapped-keys/:content_id', async (request, reply) => {
    const result = fetchWrappedKey(db, request.params.content_id);
    if (result.status === 'not_found') {
      return reply.code(404).send({ error: 'unknown or burned content_id' });
    }
    if (result.status === 'gone') {
      return reply.code(410).send({ error: 'tombstoned (past expires_at)' });
    }
    return reply.send(result.row);
  });

  return fastify;
}

// ---- entrypoint when run as a script ----
const isMain = import.meta.url === `file://${process.argv[1]}`;
if (isMain) {
  const port = Number(process.env.PORT ?? 3000);
  const dbFile = process.env.KEYSERVER_DB ?? './keyserver.db';
  const server = buildServer({ logger: true, dbFile });
  server.listen({ port, host: '127.0.0.1' }, (err) => {
    if (err) {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    }
  });
}
