import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import {
  canonicalIdentityBundleBytes,
  canonicalRolloutAdvanceBytes,
  canonicalRolloutGenesisBytes,
  deriveCanonicalOslIdentityId,
  sha256Hex,
  type CanonicalIdentityBundle,
} from "../../src/lib/identity-authority.js";
import {
  base64Encode,
  generateEd25519Pair,
  signEd25519,
  STUB_MLKEM_PUB_B64,
  STUB_RATCHET_PUB_B64,
  STUB_X25519_PUB_B64,
} from "./helpers.js";

let ipOctet = 170;

function requestId(character: string): string {
  return character.repeat(43);
}

async function identityBody(
  root: Awaited<ReturnType<typeof generateEd25519Pair>>,
  current: Awaited<ReturnType<typeof generateEd25519Pair>>,
  revision: number,
  overrides: Partial<CanonicalIdentityBundle> = {},
): Promise<Record<string, unknown>> {
  const bundle: CanonicalIdentityBundle = {
    user_id: await deriveCanonicalOslIdentityId(root.publicKeyB64),
    identity_scheme: 1,
    identity_revision: revision,
    ik_root_ed25519_pub: root.publicKeyB64,
    ik_x25519_pub: STUB_X25519_PUB_B64,
    ik_ed25519_pub: current.publicKeyB64,
    ik_mlkem768_pub: STUB_MLKEM_PUB_B64,
    ik_ratchet_initial_pub: STUB_RATCHET_PUB_B64,
    rn_capabilities: 1,
    ...overrides,
  };
  const canonical = canonicalIdentityBundleBytes(bundle);
  return {
    ...bundle,
    identity_bundle_proof_sig: await signEd25519(
      root.signingKey,
      canonical,
    ),
    registration_sig: await signEd25519(
      current.signingKey,
      canonical,
    ),
  };
}

async function register(body: Record<string, unknown>): Promise<Response> {
  return await SELF.fetch("http://test/v1/register", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-forwarded-for": `192.0.2.${ipOctet++}`,
    },
    body: JSON.stringify(body),
  });
}

describe("canonical identity and rollout authority in the shipping Worker", () => {
  it("publishes a root-authenticated full bundle and refuses substitution or revision restore", async () => {
    const root = await generateEd25519Pair();
    const current = await generateEd25519Pair();
    const firstBody = await identityBody(root, current, 1);
    expect(firstBody.user_id).toMatch(/^osl1_[a-z2-7]{52}$/);

    const substituted = {
      ...firstBody,
      ik_x25519_pub: base64Encode(new Uint8Array(32).fill(0x7f)),
    };
    const substitutionResponse = await register(substituted);
    expect(substitutionResponse.status).toBe(400);
    expect(await env.DB.prepare(
      "SELECT COUNT(*) AS count FROM users WHERE user_id = ?",
    ).bind(firstBody.user_id).first<{ count: number }>()).toEqual({ count: 0 });

    const created = await register(firstBody);
    expect(created.status).toBe(201);
    const userId = firstBody.user_id as string;
    const published = await SELF.fetch(
      `http://test/v1/pubkeys/${encodeURIComponent(userId)}`,
    );
    expect(published.status).toBe(200);
    expect(await published.json()).toMatchObject({
      user_id: userId,
      identity_scheme: 1,
      identity_revision: 1,
      ik_root_ed25519_pub: root.publicKeyB64,
      ik_x25519_pub: STUB_X25519_PUB_B64,
      ik_ed25519_pub: current.publicKeyB64,
      identity_bundle_proof_sig: firstBody.identity_bundle_proof_sig,
      registration_sig: firstBody.registration_sig,
    });

    const upperCaseId = {
      ...firstBody,
      user_id: (firstBody.user_id as string).toUpperCase(),
    };
    expect((await register(upperCaseId)).status).toBe(400);

    const next = await generateEd25519Pair();
    const secondBody = await identityBody(root, next, 2, {
      user_id: userId,
      ik_x25519_pub: base64Encode(new Uint8Array(32).fill(0x55)),
    });
    secondBody.rotation_prev_sig = await signEd25519(
      current.signingKey,
      canonicalIdentityBundleBytes(secondBody as unknown as CanonicalIdentityBundle),
    );
    expect((await register(secondBody)).status).toBe(200);

    // Caller restoration of the previously valid revision cannot restore its
    // key bundle. The durable revision remains 2 after the single refusal.
    expect((await register(firstBody)).status).toBe(409);
    const durable = await env.DB.prepare(
      `SELECT identity_revision, ik_ed25519_pub
         FROM users
        WHERE user_id = ?`,
    ).bind(userId).first<{
      identity_revision: number;
      ik_ed25519_pub: string;
    }>();
    expect(durable).toEqual({
      identity_revision: 2,
      ik_ed25519_pub: next.publicKeyB64,
    });
  });

  it("consumes D1-admin genesis once and advances the rollout root by exact CAS", async () => {
    const root = await generateEd25519Pair();
    const current = await generateEd25519Pair();
    const body = await identityBody(root, current, 1);
    expect((await register(body)).status).toBe(201);
    const userId = body.user_id as string;

    const nonce = new Uint8Array(32).fill(0x39);
    const nonceB64Url = base64Encode(nonce)
      .replaceAll("+", "-")
      .replaceAll("/", "_")
      .replace(/=+$/u, "");
    const nonceSha256 = await sha256Hex(nonce);
    const provisionedAt = Date.now() - 100;
    const admissionReceiptSha256 = "d".repeat(64);
    await env.DB.prepare(
      `INSERT INTO sender_filter_rollout_genesis
       (singleton, nonce_sha256, admission_receipt_sha256, worker_commit,
        repository_tree, keyserver_tree, provisioned_at_ms, consumed_at_ms)
       VALUES (1, ?, ?, ?, ?, ?, ?, NULL)`,
    ).bind(
      nonceSha256,
      admissionReceiptSha256,
      "1".repeat(40),
      "2".repeat(40),
      "3".repeat(40),
      provisionedAt,
    ).run();
    await expect(
      env.DB.prepare(
        `INSERT INTO sender_filter_rollout_genesis
         (singleton, nonce_sha256, admission_receipt_sha256, worker_commit,
          repository_tree, keyserver_tree, provisioned_at_ms, consumed_at_ms)
         VALUES (1, ?, ?, ?, ?, ?, ?, NULL)`,
      ).bind(
        "e".repeat(64),
        admissionReceiptSha256,
        "1".repeat(40),
        "2".repeat(40),
        "3".repeat(40),
        provisionedAt + 1,
      ).run(),
    ).rejects.toThrow();

    const genesisTimestamp = Date.now();
    const genesisRequestId = requestId("G");
    const genesisCanonical = canonicalRolloutGenesisBytes({
      root_user_id: userId,
      genesis_nonce_sha256: nonceSha256,
      timestamp_ms: genesisTimestamp,
      request_id: genesisRequestId,
    });
    const genesisRequest = {
      root_user_id: userId,
      genesis_nonce: nonceB64Url,
      timestamp_ms: genesisTimestamp,
      request_id: genesisRequestId,
      signature_b64: await signEd25519(
        root.signingKey,
        genesisCanonical,
      ),
    };
    const provisioned = await SELF.fetch(
      "http://test/v1/internal/sender-filter-rollout-root/provision",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(genesisRequest),
      },
    );
    expect(provisioned.status).toBe(201);
    expect(await provisioned.json()).toMatchObject({
      root_user_id: userId,
      capability_version: 1,
      monotonic_version: 1,
      request_id: genesisRequestId,
    });
    const consumed = await env.DB.prepare(
      `SELECT consumed_at_ms
         FROM sender_filter_rollout_genesis
        WHERE nonce_sha256 = ?`,
    ).bind(nonceSha256).first<{ consumed_at_ms: number }>();
    expect(consumed?.consumed_at_ms).toBe(genesisTimestamp);

    expect(
      (await SELF.fetch(
        "http://test/v1/internal/sender-filter-rollout-root/provision",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(genesisRequest),
        },
      )).status,
    ).toBe(409);

    const advanceTimestamp = Date.now();
    const advanceRequestId = requestId("A");
    const observation = "b".repeat(64);
    const advanceCanonical = canonicalRolloutAdvanceBytes({
      root_user_id: userId,
      expected_monotonic_version: 1,
      observation_sha256: observation,
      timestamp_ms: advanceTimestamp,
      request_id: advanceRequestId,
    });
    const advanceRequest = {
      root_user_id: userId,
      expected_monotonic_version: 1,
      observation_sha256: observation,
      timestamp_ms: advanceTimestamp,
      request_id: advanceRequestId,
      signature_b64: await signEd25519(
        root.signingKey,
        advanceCanonical,
      ),
    };
    const advanced = await SELF.fetch(
      "http://test/v1/internal/sender-filter-rollout-root/advance",
      {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(advanceRequest),
      },
    );
    expect(advanced.status).toBe(200);
    expect(await advanced.json()).toMatchObject({
      monotonic_version: 2,
      last_observation_sha256: observation,
    });
    expect(
      (await SELF.fetch(
        "http://test/v1/internal/sender-filter-rollout-root/advance",
        {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(advanceRequest),
        },
      )).status,
    ).toBe(409);
    expect(await env.DB.prepare(
      `SELECT singleton, root_user_id, root_ed25519_pub,
              identity_bundle_sha256, capability_version,
              monotonic_version, last_observation_sha256,
              provisioned_at_ms, updated_at_ms
         FROM sender_filter_rollout_root
        WHERE singleton = 1`,
    ).first()).toMatchObject({
      singleton: 1,
      root_user_id: userId,
      root_ed25519_pub: root.publicKeyB64,
      identity_bundle_sha256: expect.stringMatching(/^[0-9a-f]{64}$/),
      capability_version: 1,
      monotonic_version: 2,
      last_observation_sha256: observation,
      provisioned_at_ms: genesisTimestamp,
      updated_at_ms: advanceTimestamp,
    });
    await expect(
      env.DB.prepare(
        "DELETE FROM sender_filter_rollout_root WHERE singleton = 1",
      ).run(),
    ).rejects.toThrow(/cannot be deleted/);
    await expect(
      env.DB.prepare(
        `UPDATE sender_filter_rollout_root
            SET monotonic_version = 1
          WHERE singleton = 1`,
      ).run(),
    ).rejects.toThrow(/must advance monotonically/);
    await expect(
      env.DB.prepare(
        `UPDATE sender_filter_rollout_root
            SET root_user_id = ?
          WHERE singleton = 1`,
      ).bind("osl1_" + "a".repeat(52)).run(),
    ).rejects.toThrow(/must advance monotonically/);
    await expect(
      env.DB.prepare(
        "DELETE FROM sender_filter_rollout_genesis WHERE nonce_sha256 = ?",
      ).bind(nonceSha256).run(),
    ).rejects.toThrow(/history cannot be deleted/);
    await expect(
      env.DB.prepare(
        `UPDATE sender_filter_rollout_genesis
            SET admission_receipt_sha256 = ?
          WHERE singleton = 1`,
      ).bind("f".repeat(64)).run(),
    ).rejects.toThrow(/transition is invalid/);
    expect(await env.DB.prepare(
      `SELECT singleton, nonce_sha256, admission_receipt_sha256,
              provisioned_at_ms, consumed_at_ms
         FROM sender_filter_rollout_genesis`,
    ).all()).toMatchObject({
      results: [{
        singleton: 1,
        nonce_sha256: nonceSha256,
        admission_receipt_sha256: admissionReceiptSha256,
        provisioned_at_ms: provisionedAt,
        consumed_at_ms: genesisTimestamp,
      }],
    });
    expect(await env.DB.prepare(
      `SELECT COUNT(*) AS count
         FROM sender_filter_rollout_genesis
        WHERE admission_receipt_sha256 = ?`,
    ).bind(admissionReceiptSha256).first()).toEqual({ count: 1 });
  });
});
