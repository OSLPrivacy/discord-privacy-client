import { SELF, env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import stateCommandFixture from "../fixtures/task_3556_state_commands.json";
import {
  canonicalWrappedKeyGetBytes,
  canonicalWrappedKeyPostBytes,
} from "../../src/lib/canonical.js";
import { generateLicenseKey } from "../../src/lib/license.js";
import {
  base64Encode,
  registerTestUser,
  signEd25519,
} from "./helpers.js";

const testDb = (env as unknown as { DB: D1Database }).DB;
const HMAC = "osl-license-test-secret-v1";
const GRANT_SECONDS = 30 * 24 * 60 * 60;
const STATE_COMMANDS = stateCommandFixture.commands;

function assertListed(command: string): void {
  expect(STATE_COMMANDS).toContain(command);
  console.log(`TASK3556 state_command_list_count=${STATE_COMMANDS.length} commands=${STATE_COMMANDS.join(",")}`);
}

function uniqueId(prefix: string): string {
  return `${prefix}-${crypto.randomUUID()}`;
}

async function seedWrappedKey(): Promise<{
  contentId: string;
  recipientId: string;
  signedUrl: string;
}> {
  const senderId = uniqueId("task3556-sender");
  const recipientId = uniqueId("task3556-recipient");
  const sender = await registerTestUser(SELF, senderId);
  const recipient = await registerTestUser(SELF, recipientId);
  const contentId = uniqueId("task3556-content");
  const body = {
    content_id: contentId,
    content_type: "text",
    sender_id: senderId,
    recipient_id: recipientId,
    session_version: 1,
    share_index: 0,
    wrapped_share_blob: base64Encode(new Uint8Array([1, 2, 3, 4])),
    blob_version: 1,
    single_use: true,
    display_duration_seconds: 10,
    expires_at: new Date(Date.now() + 5 * 60_000).toISOString(),
    timestamp_ms: Date.now(),
  };
  const postBytes = canonicalWrappedKeyPostBytes({
    content_id: body.content_id,
    content_type: body.content_type,
    system_message_kind: null,
    sender_id: body.sender_id,
    recipient_id: body.recipient_id,
    session_version: body.session_version,
    share_index: body.share_index,
    wrapped_share_blob: body.wrapped_share_blob,
    blob_version: body.blob_version,
    single_use: body.single_use,
    display_duration_seconds: body.display_duration_seconds,
    expires_at: body.expires_at,
    timestamp_ms: body.timestamp_ms,
  });
  const uploaded = await SELF.fetch("http://test/v1/wrapped-keys", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      ...body,
      sender_signature_b64: await signEd25519(sender.signingKey, postBytes),
    }),
  });
  expect(uploaded.status).toBe(201);

  const ts = Date.now();
  const getBytes = canonicalWrappedKeyGetBytes({
    requester_id: recipientId,
    recipient_id: recipientId,
    content_id: contentId,
    timestamp_ms: ts,
  });
  const query = new URLSearchParams({
    requester_id: recipientId,
    recipient_id: recipientId,
    ts: String(ts),
    sig: await signEd25519(recipient.signingKey, getBytes),
  });
  return {
    contentId,
    recipientId,
    signedUrl: `http://test/v1/wrapped-keys/${encodeURIComponent(contentId)}?${query}`,
  };
}

async function readCommand(signedUrl: string): Promise<{ exitCode: number; status: number; contentBytes: number }> {
  const response = await SELF.fetch(signedUrl);
  if (response.status !== 200) {
    await response.body?.cancel();
    return { exitCode: 1, status: response.status, contentBytes: 0 };
  }
  const body = await response.arrayBuffer();
  return { exitCode: 0, status: response.status, contentBytes: body.byteLength };
}

async function consumingGetReceiptCount(contentId: string): Promise<number> {
  const row = await testDb.prepare(
    "SELECT COUNT(*) AS count FROM consuming_get_receipts WHERE target_id = ?",
  ).bind(contentId).first<{ count: number }>();
  return row?.count ?? 0;
}

async function seedRedeemableLicense(): Promise<{ plaintext: string; hash: string }> {
  const { plaintext, hash } = await generateLicenseKey(HMAC);
  const subscriptionId = uniqueId("task3556-sub");
  await env.DB.batch([
    env.DB.prepare(
      `INSERT INTO subscriptions (subscription_id, customer_id, customer_email,
        status, current_period_end, cancel_at_period_end, created_at, updated_at)
       VALUES (?, 'cus_task3556', '', 'ACTIVE', NULL, 0, 1, 1)`,
    ).bind(subscriptionId),
    env.DB.prepare(
      `INSERT INTO licenses (license_hash, subscription_id, issued_at, grant_seconds)
       VALUES (?, ?, 1, ?)`,
    ).bind(hash, subscriptionId, GRANT_SECONDS),
  ]);
  return { plaintext, hash };
}

async function redeemCommand(licenseKey: string): Promise<{ status: number; body: Record<string, unknown> }> {
  const response = await SELF.fetch("http://test/v1/license/redeem", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ license_key: licenseKey }),
  });
  return {
    status: response.status,
    body: await response.json() as Record<string, unknown>,
  };
}

async function redeemedLicenseCount(hash: string): Promise<number> {
  const row = await env.DB.prepare(
    `SELECT COUNT(*) AS count FROM licenses
      WHERE license_hash = ? AND redeemed_at IS NOT NULL AND expires_at IS NOT NULL`,
  ).bind(hash).first<{ count: number }>();
  return row?.count ?? 0;
}

describe("TASK 3556 repeated state commands", () => {
  it("checks exact-repeat wrapped-key and license commands sequentially and concurrently", async () => {
    assertListed("GET /v1/wrapped-keys/:content_id");
    assertListed("POST /v1/license/redeem");

    const sequentialWrapped = await seedWrappedKey();
    const firstRead = await readCommand(sequentialWrapped.signedUrl);
    const secondRead = await readCommand(sequentialWrapped.signedUrl);
    const sequentialReceiptCount = await consumingGetReceiptCount(sequentialWrapped.contentId);
    expect(firstRead.exitCode).toBe(0);
    expect(firstRead.status).toBe(200);
    expect(firstRead.contentBytes).toBeGreaterThan(0);
    expect(secondRead.exitCode).toBe(1);
    expect(secondRead.status).toBe(409);
    expect(secondRead.contentBytes).toBe(0);
    expect(sequentialReceiptCount).toBe(1);
    console.log(
      `TASK3556 command=GET /v1/wrapped-keys/:content_id mode=sequential intended_state_changes=${sequentialReceiptCount} second_result=refusal:signed consuming GET already used`,
    );

    const concurrentWrapped = await seedWrappedKey();
    const concurrentReads = await Promise.all([
      readCommand(concurrentWrapped.signedUrl),
      readCommand(concurrentWrapped.signedUrl),
    ]);
    const concurrentReceiptCount = await consumingGetReceiptCount(concurrentWrapped.contentId);
    expect(concurrentReads.filter((result) => result.status === 200)).toHaveLength(1);
    expect(concurrentReads.filter((result) => result.status === 409)).toHaveLength(1);
    expect(concurrentReceiptCount).toBe(1);
    console.log(
      `TASK3556 command=GET /v1/wrapped-keys/:content_id mode=concurrent intended_state_changes=${concurrentReceiptCount} second_result=refusal:signed consuming GET already used`,
    );

    const sequentialLicense = await seedRedeemableLicense();
    const firstRedeem = await redeemCommand(sequentialLicense.plaintext);
    await new Promise((resolve) => setTimeout(resolve, 1_100));
    const secondRedeem = await redeemCommand(sequentialLicense.plaintext);
    const sequentialLicenseCount = await redeemedLicenseCount(sequentialLicense.hash);
    expect(firstRedeem.status).toBe(200);
    expect(secondRedeem.status).toBe(200);
    expect(secondRedeem.body).toEqual(firstRedeem.body);
    expect(sequentialLicenseCount).toBe(1);
    console.log(
      `TASK3556 command=POST /v1/license/redeem mode=sequential intended_state_changes=${sequentialLicenseCount} second_result=safe_repeat_same_redemption`,
    );

    const concurrentLicense = await seedRedeemableLicense();
    const concurrentRedeems = await Promise.all([
      redeemCommand(concurrentLicense.plaintext),
      redeemCommand(concurrentLicense.plaintext),
    ]);
    const concurrentLicenseCount = await redeemedLicenseCount(concurrentLicense.hash);
    expect(concurrentRedeems.every((result) => result.status === 200)).toBe(true);
    expect(concurrentRedeems[1]?.body).toEqual(concurrentRedeems[0]?.body);
    expect(concurrentLicenseCount).toBe(1);
    console.log(
      `TASK3556 command=POST /v1/license/redeem mode=concurrent intended_state_changes=${concurrentLicenseCount} second_result=safe_repeat_same_redemption`,
    );
  });
});
