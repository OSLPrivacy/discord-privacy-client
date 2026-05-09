// Key server for discord-privacy-client.
//
// Phase B (closed-beta deployable): pre-shared admin token gates
// state-mutating routes; user-id allowlist gates registration; rate
// limiting on mutations. Local-dev mode preserved: when no env vars
// are set, behaviour is identical to the v0.0.1 prototype.
//
// What this is NOT: a TLS-terminating service (deploy behind
// Cloudflare/Railway TLS), a Discord-OAuth-gated service (closed
// beta = trusted-token model), or signature-verifying on register
// (the prototype identity-key signature stays mocked client-side).
// v1 stable still replaces this with the OAuth-gated service in
// `docs/design/key-server-api.md`.
//
// Endpoints:
//   POST   /v1/register                  [admin token + allowlist]
//   GET    /v1/pubkeys/:user_id          [public]
//   POST   /v1/wrapped-keys              [admin token]
//   GET    /v1/wrapped-keys/:content_id  [public]
//   DELETE /v1/wrapped-keys              [admin token + Ed25519 sig]
//   GET    /v1/prekey-bundle/:user_id    [public]
//   POST   /v1/prekey-bundle/replenish   [admin token + Ed25519 sig]
//   GET    /v1/selector-manifest         [public]
//   GET    /v1/healthz                   [public]
//
// "Public" means no admin token; the GET endpoints serve public
// keys + read-only state, which is the design intent (anyone with
// a user_id should be able to look up the recipient's public keys
// to encrypt to them).
//
// Endpoints still deferred from the design doc: sessions/rotate,
// tokens/issue.

import { createHash, timingSafeEqual } from 'node:crypto';
import Fastify from 'fastify';
import rateLimitPlugin from '@fastify/rate-limit';
import {
  openDatabase,
  upsertUser,
  getUser,
  insertWrappedKey,
  fetchWrappedKey,
  upsertPrekeyBundle,
  popPrekeyBundle,
  burnWrappedKeys,
} from './db.js';
import {
  canonicalBurnBytes,
  canonicalReplenishBytes,
  verifyEd25519,
} from './canonical.js';

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

// Constant-time comparison. Hashes both sides to SHA-256 first so the
// length of `a` (the secret) doesn't leak via the comparator's
// length precondition. Both digests are 32 bytes regardless of
// input length.
function constantTimeTokenEqual(provided, expected) {
  const hp = createHash('sha256').update(provided, 'utf8').digest();
  const he = createHash('sha256').update(expected, 'utf8').digest();
  return timingSafeEqual(hp, he);
}

// Build the admin-token preHandler. Returns `null` when no token is
// configured (dev mode — passes through every request).
//
// The hook logs the rejected `attempted_user_id` (when the body
// carries one) so operators monitoring the keyserver have a
// fingerprint of who tried to forge a registration. The header
// itself is NEVER logged — that would land secrets in operator
// telemetry.
function buildAdminAuthHook(adminToken) {
  if (!adminToken) return null;
  return async function adminAuthHook(request, reply) {
    const header = request.headers.authorization;
    let provided = '';
    if (typeof header === 'string') {
      const m = header.match(/^Bearer\s+(.+)$/i);
      if (m) provided = m[1].trim();
    }
    // We always run the hash + comparison even on empty `provided`
    // so a totally-missing header doesn't take a different timing
    // path from a present-but-wrong header.
    const ok = provided.length > 0 && constantTimeTokenEqual(provided, adminToken);
    if (!ok) {
      request.log.warn(
        {
          url: request.url,
          method: request.method,
          attempted_user_id:
            (request.body && typeof request.body === 'object'
              ? request.body.user_id
              : null) ?? null,
          had_header: header != null,
        },
        'admin token check failed'
      );
      return reply.code(401).send({ error: 'unauthorized' });
    }
  };
}

/**
 * Build a configured (but not-yet-listening) Fastify instance.
 *
 * **Async** because the rate-limit plugin must finish registering
 * before any route is added — otherwise its `onRoute` hook won't
 * fire and per-route `config.rateLimit` is silently ignored. The
 * old (Phase A) signature was sync; callers updating from prototype
 * code need `await buildServer(...)`.
 */
export async function buildServer({
  logger = false,
  dbFile = ':memory:',
  // Pre-signed selector-manifest envelope JSON (the SignedManifest
  // shape from `crates/selectors/src/manifest.rs`). When unset, the
  // /v1/selector-manifest endpoint replies 503 — clients fail closed
  // through the CDN-mirror fallback path. The signing key is offline
  // in production; this server only ever serves the bytes.
  selectorManifest = null,
  // Pre-shared admin token. State-mutating routes (POST /register,
  // POST /wrapped-keys, POST /prekey-bundle/replenish, DELETE
  // /wrapped-keys) require `Authorization: Bearer <token>` matching
  // this value. When `null`/empty (the default), no auth is enforced
  // — preserves the local-dev `npm start` workflow. Production sets
  // this from `OSL_KEYSERVER_ADMIN_TOKEN` env var.
  adminToken = null,
  // User-id allowlist for /v1/register. When non-empty, only listed
  // user_ids may register; others get 403. When `null`/empty, no
  // allowlist enforcement. Defence-in-depth: even if the admin
  // token leaks, the attacker still needs to know an allowlisted
  // user_id to forge an identity. Production sets this from
  // `OSL_KEYSERVER_ALLOWED_USERS` (comma-separated).
  allowedUsers = null,
  // Rate-limit configuration for mutation routes. Set to e.g.
  // `{ max: 10, timeWindow: '1 minute' }` to enable. `false` (the
  // default) skips plugin registration entirely so tests don't
  // trip the limiter on rapid `server.inject` calls.
  rateLimit = false,
} = {}) {
  const fastify = Fastify({ logger });
  const db = openDatabase(dbFile);

  // Normalise inputs once so per-route logic doesn't re-check
  // emptiness/types each time.
  const effectiveAdminToken =
    typeof adminToken === 'string' && adminToken.length > 0 ? adminToken : null;
  const effectiveAllowedUsers =
    Array.isArray(allowedUsers) && allowedUsers.length > 0 ? allowedUsers : null;

  if (!effectiveAdminToken) {
    // Surface dev-mode at startup so operators know the server is
    // running unauthenticated. Logger may be disabled in tests; the
    // `request.log` and `fastify.log` paths are NoOps in that case.
    fastify.log.warn(
      'OSL keyserver: ADMIN TOKEN UNSET — state-mutating endpoints \
are open. OK for localhost dev; DO NOT do this on a public host.'
    );
  }

  // Register rate-limit plugin globally with `global: false` so
  // routes opt-in via `config.rateLimit`. Skipping registration
  // entirely (instead of a no-op config) keeps the default-test
  // setup zero-overhead and avoids the plugin's onRequest hook.
  //
  // Pass max/timeWindow as plugin-register defaults so the
  // `onRoute` hook (set up at register time) carries them; the
  // per-route `config.rateLimit` block can override but doesn't
  // need to. Crucially: this register call MUST be awaited before
  // any route is added — fastify-plugin installs `onRoute` on
  // load, and routes registered before then bypass the limiter.
  if (rateLimit) {
    const limitOpts = typeof rateLimit === 'object' ? rateLimit : {};
    await fastify.register(rateLimitPlugin, {
      global: false,
      ...limitOpts,
    });
  }

  // Pre-built per-route options for state-mutating endpoints.
  // Computed once; same object reused across registrations.
  const adminAuthHook = buildAdminAuthHook(effectiveAdminToken);
  const mutationRouteOpts = {};
  if (adminAuthHook) mutationRouteOpts.preHandler = adminAuthHook;
  if (rateLimit) mutationRouteOpts.config = { rateLimit };

  fastify.addHook('onClose', async () => {
    db.close();
  });

  fastify.get('/v1/healthz', async () => ({ ok: true }));

  // ---- POST /v1/register ----
  fastify.post('/v1/register', mutationRouteOpts, async (request, reply) => {
    const body = request.body ?? {};
    const required = [
      'user_id',
      'ik_x25519_pub',
      'ik_ed25519_pub',
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
    // Allowlist check: post-token, pre-field-validation. We
    // already know the caller has the admin token (the preHandler
    // ran), so allowlist failure logs the *successful-token*
    // attempt — useful operator signal that the token may have
    // leaked.
    if (effectiveAllowedUsers && !effectiveAllowedUsers.includes(body.user_id)) {
      request.log.warn(
        { attempted_user_id: body.user_id },
        'register allowlist check failed (token was valid)'
      );
      return reply.code(403).send({ error: 'forbidden: user_id not on allowlist' });
    }
    for (const k of [
      'ik_x25519_pub',
      'ik_ed25519_pub',
      'ik_mlkem768_pub',
      'ik_x25519_signature',
    ]) {
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
  fastify.post('/v1/wrapped-keys', mutationRouteOpts, async (request, reply) => {
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

  // ---- GET /v1/prekey-bundle/:user_id ----
  fastify.get('/v1/prekey-bundle/:user_id', async (request, reply) => {
    const bundle = popPrekeyBundle(db, request.params.user_id);
    if (!bundle) {
      return reply
        .code(404)
        .send({ error: 'unknown user_id or no prekey bundle uploaded' });
    }
    // `opk: null` is the no-OPK fallback (per the design doc's
    // "OPK exhaustion fallback").
    return reply.send(bundle);
  });

  // ---- POST /v1/prekey-bundle/replenish ----
  //
  // Body:
  //   {
  //     user_id, batch_signature_b64,
  //     spk?: { pub_b64, signature_b64, rotated_at },
  //     opks: [ { id, pub_b64 }, ... ]
  //   }
  //
  // The batch_signature is Ed25519 over `canonicalReplenishBytes(...)`
  // by the user's IK_Ed25519. Server verifies before mutating state.
  fastify.post('/v1/prekey-bundle/replenish', mutationRouteOpts, async (request, reply) => {
    const b = request.body ?? {};
    if (!isPlainString(b.user_id)) {
      return reply.code(400).send({ error: 'user_id required' });
    }
    if (!isNonEmptyBase64(b.batch_signature_b64)) {
      return reply.code(400).send({ error: 'batch_signature_b64 required' });
    }
    if (!Array.isArray(b.opks)) {
      return reply.code(400).send({ error: 'opks must be an array' });
    }
    for (const o of b.opks) {
      if (!Number.isInteger(o?.id) || o.id < 0) {
        return reply.code(400).send({ error: 'opk.id must be u32' });
      }
      if (!isNonEmptyBase64(o?.pub_b64)) {
        return reply.code(400).send({ error: 'opk.pub_b64 must be base64' });
      }
    }
    if (b.spk != null) {
      if (
        !isNonEmptyBase64(b.spk.pub_b64) ||
        !isNonEmptyBase64(b.spk.signature_b64) ||
        !isPlainString(b.spk.rotated_at) ||
        Number.isNaN(Date.parse(b.spk.rotated_at))
      ) {
        return reply.code(400).send({ error: 'spk fields malformed' });
      }
    }
    const user = getUser(db, b.user_id);
    if (!user) {
      return reply
        .code(404)
        .send({ error: 'unknown user_id — register before replenish' });
    }
    const message = canonicalReplenishBytes({
      user_id: b.user_id,
      spk: b.spk ?? null,
      opks: b.opks,
    });
    const ikEd25519 = Buffer.from(user.ik_ed25519_pub, 'base64');
    const sig = Buffer.from(b.batch_signature_b64, 'base64');
    if (!verifyEd25519(ikEd25519, message, sig)) {
      return reply
        .code(401)
        .send({ error: 'batch_signature_b64 verification failed' });
    }
    try {
      upsertPrekeyBundle(db, b.user_id, b.spk ?? null, b.opks);
    } catch (err) {
      if (err.code === 'SQLITE_CONSTRAINT_PRIMARYKEY') {
        return reply.code(409).send({ error: 'opk id already used' });
      }
      throw err;
    }
    return reply.code(200).send({
      user_id: b.user_id,
      opks_added: b.opks.length,
    });
  });

  // ---- DELETE /v1/wrapped-keys ----
  //
  // Body:
  //   {
  //     scope: "single" | "to_user" | "all",
  //     user_id,                       // burning user
  //     target_content_id?,            // iff scope == "single"
  //     target_user_id?,               // iff scope == "to_user"
  //     burn_signature_b64,            // Ed25519 over canonicalBurnBytes
  //   }
  //
  // The burn signature is over the canonical encoding of
  // (user_id, scope, target). Server verifies against the user's
  // stored IK_Ed25519. Only rows where `sender_id == user_id` are
  // deleted — you can't burn another user's content.
  fastify.delete('/v1/wrapped-keys', mutationRouteOpts, async (request, reply) => {
    const b = request.body ?? {};
    if (!isPlainString(b.scope) || !['single', 'to_user', 'all'].includes(b.scope)) {
      return reply
        .code(400)
        .send({ error: 'scope must be "single" | "to_user" | "all"' });
    }
    if (!isPlainString(b.user_id)) {
      return reply.code(400).send({ error: 'user_id required' });
    }
    if (!isNonEmptyBase64(b.burn_signature_b64)) {
      return reply.code(400).send({ error: 'burn_signature_b64 required' });
    }
    let target;
    if (b.scope === 'single') {
      if (!isPlainString(b.target_content_id)) {
        return reply
          .code(400)
          .send({ error: 'target_content_id required for scope=single' });
      }
      target = { content_id: b.target_content_id };
    } else if (b.scope === 'to_user') {
      if (!isPlainString(b.target_user_id)) {
        return reply
          .code(400)
          .send({ error: 'target_user_id required for scope=to_user' });
      }
      target = { user_id: b.target_user_id };
    } else {
      target = null;
      if (b.target_content_id != null || b.target_user_id != null) {
        return reply
          .code(400)
          .send({ error: 'scope=all rejects target fields' });
      }
    }
    const user = getUser(db, b.user_id);
    if (!user) {
      return reply
        .code(404)
        .send({ error: 'unknown user_id — register before burn' });
    }
    const message = canonicalBurnBytes({
      user_id: b.user_id,
      scope: b.scope,
      target: target ?? undefined,
    });
    const ikEd25519 = Buffer.from(user.ik_ed25519_pub, 'base64');
    const sig = Buffer.from(b.burn_signature_b64, 'base64');
    if (!verifyEd25519(ikEd25519, message, sig)) {
      return reply
        .code(401)
        .send({ error: 'burn_signature_b64 verification failed' });
    }
    const { deleted_count } = burnWrappedKeys(db, b.user_id, b.scope, target);
    return reply.send({
      scope: b.scope,
      deleted_count,
    });
  });

  // ---- GET /v1/selector-manifest ----
  //
  // Returns the SignedManifest envelope verbatim. Clients verify the
  // Ed25519 signature against their hard-coded release key — the
  // server is *not* a trust anchor, just a delivery channel. When
  // unconfigured, returns 503 so clients can fall through to the CDN
  // mirror (`docs/design/key-server-api.md` § "Manifest mirror").
  fastify.get('/v1/selector-manifest', async (request, reply) => {
    if (!selectorManifest) {
      return reply
        .code(503)
        .send({ error: 'selector manifest not configured on this keyserver' });
    }
    return reply.send(selectorManifest);
  });

  return fastify;
}

// ---- entrypoint when run as a script ----
import { pathToFileURL } from 'node:url';
const isMain = import.meta.url === pathToFileURL(process.argv[1]).href;
if (isMain) {
  const port = Number(process.env.PORT ?? 3000);
  // Default loopback for local dev. Set HOST=0.0.0.0 (or '::') in
  // production to bind all interfaces — required for Railway / any
  // container hosting where the platform reverse-proxy talks to
  // the app over the internal network.
  const host = process.env.HOST ?? '127.0.0.1';
  const dbFile = process.env.KEYSERVER_DB ?? './keyserver.db';
  // SELECTOR_MANIFEST_PATH points at a JSON file holding the
  // SignedManifest envelope. Optional — without it the
  // /v1/selector-manifest endpoint returns 503 and clients fall
  // through to the CDN mirror.
  let selectorManifest = null;
  if (process.env.SELECTOR_MANIFEST_PATH) {
    const fs = await import('node:fs/promises');
    const txt = await fs.readFile(process.env.SELECTOR_MANIFEST_PATH, 'utf8');
    selectorManifest = JSON.parse(txt);
  }

  // Auth + allowlist + rate-limit configuration. All optional;
  // unset env vars revert to local-dev no-auth behaviour.
  const adminToken = process.env.OSL_KEYSERVER_ADMIN_TOKEN || null;
  const allowedUsers = process.env.OSL_KEYSERVER_ALLOWED_USERS
    ? process.env.OSL_KEYSERVER_ALLOWED_USERS.split(',')
        .map((s) => s.trim())
        .filter((s) => s.length > 0)
    : null;

  // Rate limit applies to mutation routes only (see the per-route
  // `mutationRouteOpts` in `buildServer`). Defaults are sized for
  // closed-beta dogfood scale: 10 req/min/IP per mutation route.
  // Production tuning would land here.
  const rateLimit = adminToken
    ? { max: 10, timeWindow: '1 minute' }
    : false;

  const server = await buildServer({
    logger: true,
    dbFile,
    selectorManifest,
    adminToken,
    allowedUsers,
    rateLimit,
  });
  server.listen({ port, host }, (err, address) => {
    if (err) {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    }
    server.log.info(
      {
        address,
        admin_auth: adminToken ? 'enabled' : 'DISABLED (dev mode)',
        allowlist: allowedUsers ? allowedUsers : 'disabled',
        rate_limit: rateLimit ? rateLimit : 'disabled',
      },
      'OSL keyserver listening'
    );
  });
}
