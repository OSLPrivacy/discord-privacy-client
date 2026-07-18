import { validateDeliveryPublicKey } from "./anonymous-crypto.js";
import { generateLicenseKey, type LicenseIssuer } from "./license.js";

const MAX_COMP_BATCH = 25;
const MIN_EXPIRY_SECONDS = 60 * 60;
const MAX_EXPIRY_SECONDS = 366 * 24 * 60 * 60;

export interface CompBatchInput {
  quantity: number;
  purpose: string;
  expiresAt: number;
  requestId: string;
  deliveryPublicKeySpki: string;
  issuer: LicenseIssuer;
  licenseHmacSecret: string;
  auditHmacSecret: string;
}

export interface EncryptedCompDelivery {
  algorithm: "rsa-oaep-sha256+aes-256-gcm";
  wrapped_key: string;
  nonce: string;
  ciphertext: string;
}

export interface IssuedCompBatch {
  batchId: string;
  quantity: number;
  expiresAt: number;
  auditDigest: string;
  delivery: EncryptedCompDelivery;
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function randomHex(bytes: number): string {
  const value = new Uint8Array(bytes);
  crypto.getRandomValues(value);
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function hmacHex(secret: string, value: string): Promise<string> {
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const digest = new Uint8Array(await crypto.subtle.sign(
    "HMAC",
    key,
    new TextEncoder().encode(value),
  ));
  return Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function encryptDelivery(
  publicKeySpki: string,
  payload: unknown,
): Promise<EncryptedCompDelivery> {
  const rsaKey = await validateDeliveryPublicKey(publicKeySpki);
  const aesKey = await crypto.subtle.generateKey(
    { name: "AES-GCM", length: 256 },
    true,
    ["encrypt"],
  ) as CryptoKey;
  const rawKey = new Uint8Array(
    await crypto.subtle.exportKey("raw", aesKey) as ArrayBuffer,
  );
  const nonce = new Uint8Array(12);
  crypto.getRandomValues(nonce);
  const aad = new TextEncoder().encode("osl-comp-delivery-v1");
  const plaintext = new TextEncoder().encode(JSON.stringify(payload));
  const [wrappedKey, ciphertext] = await Promise.all([
    crypto.subtle.encrypt({ name: "RSA-OAEP" }, rsaKey, rawKey),
    crypto.subtle.encrypt({ name: "AES-GCM", iv: nonce, additionalData: aad }, aesKey, plaintext),
  ]);
  rawKey.fill(0);
  plaintext.fill(0);
  return {
    algorithm: "rsa-oaep-sha256+aes-256-gcm",
    wrapped_key: base64(new Uint8Array(wrappedKey)),
    nonce: base64(nonce),
    ciphertext: base64(new Uint8Array(ciphertext)),
  };
}

export function validateCompBatchInput(input: CompBatchInput, now: number): void {
  if (!Number.isSafeInteger(input.quantity) || input.quantity < 1 || input.quantity > MAX_COMP_BATCH) {
    throw new Error("quantity must be an integer from 1 to 25");
  }
  if (input.purpose.length < 3 || input.purpose.length > 200) {
    throw new Error("purpose must contain 3 to 200 characters");
  }
  if (!/^[A-Za-z0-9_-]{43}$/.test(input.requestId)) {
    throw new Error("request_id must be base64url(32 bytes)");
  }
  if (
    !Number.isSafeInteger(input.expiresAt) ||
    input.expiresAt < now + MIN_EXPIRY_SECONDS ||
    input.expiresAt > now + MAX_EXPIRY_SECONDS
  ) {
    throw new Error("expires_at must be between 1 hour and 366 days from now");
  }
}

export async function issueCompBatch(
  db: D1Database,
  input: CompBatchInput,
): Promise<IssuedCompBatch> {
  const now = Math.floor(Date.now() / 1000);
  validateCompBatchInput(input, now);
  await validateDeliveryPublicKey(input.deliveryPublicKeySpki);

  const requestHash = await hmacHex(
    input.auditHmacSecret,
    `osl-comp-request-v1\0${input.requestId}`,
  );
  const existing = await db.prepare(
    "SELECT batch_id FROM comp_batches WHERE request_hash = ?",
  ).bind(requestHash).first<{ batch_id: string }>();
  if (existing) throw new Error("request_id has already been used");

  const purposeHash = await hmacHex(
    input.auditHmacSecret,
    `osl-comp-purpose-v1\0${input.purpose}`,
  );
  const batchId = `comp_${randomHex(16)}`;
  const auditDigest = await hmacHex(
    input.auditHmacSecret,
    ["osl-comp-audit-v1", batchId, input.issuer, purposeHash, input.quantity, input.expiresAt, now].join("\0"),
  );

  const codes: string[] = [];
  const statements: D1PreparedStatement[] = [
    db.prepare(
      `INSERT INTO comp_batches (
         batch_id, request_hash, issuer, purpose_hash, quantity,
         expires_at, issued_at, revoked_at, audit_digest
       ) VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?)`,
    ).bind(
      batchId,
      requestHash,
      input.issuer,
      purposeHash,
      input.quantity,
      input.expiresAt,
      now,
      auditDigest,
    ),
  ];

  for (let ordinal = 1; ordinal <= input.quantity; ordinal += 1) {
    const license = await generateLicenseKey(input.licenseHmacSecret, input.issuer);
    const subscriptionId = `${batchId}_${ordinal.toString().padStart(2, "0")}`;
    codes.push(license.plaintext);
    statements.push(
      db.prepare(
        `INSERT INTO subscriptions (
           subscription_id, customer_id, customer_email, status,
           current_period_end, cancel_at_period_end, created_at, updated_at, is_comp
         ) VALUES (?, ?, '', 'ACTIVE', ?, 0, ?, ?, 1)`,
      ).bind(subscriptionId, `comp:${batchId}`, input.expiresAt, now, now),
      db.prepare(
        `INSERT INTO licenses (
           license_hash, subscription_id, issued_at, revoked_at, revoked_reason
         ) VALUES (?, ?, ?, NULL, NULL)`,
      ).bind(license.hash, subscriptionId, now),
      db.prepare(
        `INSERT INTO comp_batch_licenses (
           batch_id, license_hash, subscription_id, ordinal
         ) VALUES (?, ?, ?, ?)`,
      ).bind(batchId, license.hash, subscriptionId, ordinal),
    );
  }

  const delivery = await encryptDelivery(input.deliveryPublicKeySpki, {
    version: 1,
    batch_id: batchId,
    issuer: input.issuer,
    expires_at: input.expiresAt,
    activation_codes: codes,
  });
  await db.batch(statements);
  codes.fill("");
  return { batchId, quantity: input.quantity, expiresAt: input.expiresAt, auditDigest, delivery };
}

export async function revokeCompBatch(db: D1Database, batchId: string): Promise<boolean> {
  const now = Math.floor(Date.now() / 1000);
  const results = await db.batch([
    db.prepare(
      `UPDATE licenses SET revoked_at = ?, revoked_reason = 'manual'
        WHERE revoked_at IS NULL AND license_hash IN (
          SELECT license_hash FROM comp_batch_licenses WHERE batch_id = ?
        )`,
    ).bind(now, batchId),
    db.prepare(
      `UPDATE subscriptions SET status = 'REVOKED', updated_at = ?
        WHERE subscription_id IN (
          SELECT subscription_id FROM comp_batch_licenses WHERE batch_id = ?
        )`,
    ).bind(now, batchId),
    db.prepare(
      "UPDATE comp_batches SET revoked_at = COALESCE(revoked_at, ?) WHERE batch_id = ?",
    ).bind(now, batchId),
  ]);
  return (results.at(-1)?.meta?.changes ?? 0) === 1;
}
