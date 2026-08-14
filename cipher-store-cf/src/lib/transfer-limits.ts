/// Per-client transfer limits for ciphertext bodies.
///
/// A "person" is the client address, matching the store's existing abuse
/// controls. The address is HMACed before it reaches D1. Upload slots are a
/// strict, atomic ceiling; bandwidth is a shared, one-second byte budget. A
/// budget miss waits for the next window and retries, so a fast client is
/// slowed instead of having a partially transferred object cut off.

import type { Env } from "../env.js";
import { error } from "./http.js";

export const MAX_CONCURRENT_UPLOADS_PER_CLIENT = 3;
export const UPLOAD_BYTES_PER_SECOND = 1024 * 1024;
export const DOWNLOAD_BYTES_PER_SECOND = 4 * 1024 * 1024;

const SLOT_LEASE_MS = 10 * 60 * 1000;
const WINDOW_MS = 1000;

type Direction = "upload" | "download";
type Now = () => number;
type Sleep = (milliseconds: number) => Promise<void>;

interface StreamLimitOptions {
  bytesPerSecond: number;
  now?: Now;
  sleep?: Sleep;
}

function defaultSleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function randomHex(bytes: number): string {
  const value = new Uint8Array(bytes);
  crypto.getRandomValues(value);
  return Array.from(value, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function clientKey(env: Env, clientIp: string): Promise<string> {
  if (env.RATE_LIMIT_HASH_KEY.length < 32) {
    throw new Error("rate-limit hash key unavailable");
  }
  const encoder = new TextEncoder();
  const key = await crypto.subtle.importKey(
    "raw",
    encoder.encode(env.RATE_LIMIT_HASH_KEY),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  const signature = new Uint8Array(await crypto.subtle.sign(
    "HMAC",
    key,
    encoder.encode(`transfer-v1|${clientIp}`),
  ));
  return Array.from(signature, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function acquireUploadSlot(env: Env, clientIp: string, now: number): Promise<{ clientKey: string; slotId: string } | null> {
  const key = await clientKey(env, clientIp);
  // A worker killed while reading a body cannot run its finally block. Pruning
  // the caller's expired leases before the atomic admission prevents that
  // failure from permanently denying uploads.
  await env.DB.prepare(
    "DELETE FROM transfer_upload_slots WHERE client_key = ? AND expires_at <= ?",
  ).bind(key, now).run();
  const slotId = randomHex(16);
  const inserted = await env.DB.prepare(
    `INSERT INTO transfer_upload_slots (client_key, slot_id, expires_at)
     SELECT ?, ?, ?
      WHERE (SELECT COUNT(*) FROM transfer_upload_slots
              WHERE client_key = ? AND expires_at > ?) < ?`,
  ).bind(key, slotId, now + SLOT_LEASE_MS, key, now, MAX_CONCURRENT_UPLOADS_PER_CLIENT).run();
  return (inserted.meta.changes ?? 0) === 1 ? { clientKey: key, slotId } : null;
}

async function releaseUploadSlot(env: Env, slot: { clientKey: string; slotId: string }): Promise<void> {
  await env.DB.prepare(
    "DELETE FROM transfer_upload_slots WHERE client_key = ? AND slot_id = ?",
  ).bind(slot.clientKey, slot.slotId).run();
}

async function reserveWindowBytes(
  env: Env,
  key: string,
  direction: Direction,
  windowStart: number,
  bytes: number,
  limit: number,
): Promise<boolean> {
  const admitted = await env.DB.prepare(
    `INSERT INTO transfer_bandwidth_windows
       (client_key, direction, window_start_ms, used_bytes)
     VALUES (?, ?, ?, ?)
     ON CONFLICT(client_key, direction, window_start_ms) DO UPDATE
       SET used_bytes = used_bytes + excluded.used_bytes
       WHERE transfer_bandwidth_windows.used_bytes + excluded.used_bytes <= ?
     RETURNING used_bytes`,
  ).bind(key, direction, windowStart, bytes, limit).first<{ used_bytes: number }>();
  return admitted !== null;
}

/**
 * Return a backpressure-preserving stream whose bytes consume one shared,
 * per-client budget. `now` and `sleep` are seams for deterministic tests; the
 * production path is wall time plus setTimeout.
 */
export function limitTransferStream(
  source: ReadableStream<Uint8Array>,
  env: Env,
  clientIp: string,
  direction: Direction,
  options: StreamLimitOptions,
): ReadableStream<Uint8Array> {
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? defaultSleep;
  const reader = source.getReader();
  let keyPromise: Promise<string> | undefined;

  async function permit(bytes: number): Promise<void> {
    // No single D1 reservation may be larger than the budget. Splitting a
    // large R2 chunk makes it consume consecutive windows rather than turning
    // an otherwise valid download into an error.
    const key = keyPromise ??= clientKey(env, clientIp);
    while (true) {
      const current = now();
      const windowStart = current - (current % WINDOW_MS);
      if (await reserveWindowBytes(env, await key, direction, windowStart, bytes, options.bytesPerSecond)) return;
      await sleep(Math.max(1, windowStart + WINDOW_MS - current));
    }
  }

  let pending: Uint8Array | undefined;
  let offset = 0;
  return new ReadableStream<Uint8Array>({
    async pull(controller) {
      try {
        if (!pending || offset === pending.byteLength) {
          const next = await reader.read();
          if (next.done) {
            controller.close();
            return;
          }
          if (!next.value || next.value.byteLength === 0) return;
          pending = next.value;
          offset = 0;
        }
        const bytes = Math.min(options.bytesPerSecond, pending.byteLength - offset);
        await permit(bytes);
        controller.enqueue(pending.slice(offset, offset + bytes));
        offset += bytes;
      } catch (cause) {
        await reader.cancel(cause).catch(() => undefined);
        controller.error(cause);
      }
    },
    async cancel(reason) {
      await reader.cancel(reason);
    },
  });
}

/** Acquire one upload slot, pace the request body, and always release it. */
export async function withUploadTransferLimit(
  request: Request,
  env: Env,
  clientIp: string,
  handler: (limitedRequest: Request) => Promise<Response>,
): Promise<Response> {
  const slot = await acquireUploadSlot(env, clientIp, Date.now());
  if (!slot) {
    return error(
      429,
      "upload_concurrency_limit",
      `a client may run at most ${MAX_CONCURRENT_UPLOADS_PER_CLIENT} uploads at once`,
    );
  }
  try {
    const limitedRequest = request.body
      ? new Request(request, {
        body: limitTransferStream(request.body, env, clientIp, "upload", {
          bytesPerSecond: UPLOAD_BYTES_PER_SECOND,
        }),
      })
      : request;
    return await handler(limitedRequest);
  } finally {
    await releaseUploadSlot(env, slot);
  }
}

/** Pace opaque successful download bytes without changing their response. */
export function limitDownloadResponse(response: Response, env: Env, clientIp: string): Response {
  if (!response.body || response.status < 200 || response.status >= 300) return response;
  return new Response(
    limitTransferStream(response.body, env, clientIp, "download", {
      bytesPerSecond: DOWNLOAD_BYTES_PER_SECOND,
    }),
    { status: response.status, statusText: response.statusText, headers: response.headers },
  );
}

/** Called by the existing five-minute sweep to bound opaque limiter state. */
export async function sweepTransferLimits(env: Env): Promise<void> {
  const now = Date.now();
  await env.DB.prepare(
    "DELETE FROM transfer_upload_slots WHERE expires_at <= ?",
  ).bind(now).run();
  await env.DB.prepare(
    "DELETE FROM transfer_bandwidth_windows WHERE window_start_ms < ?",
  ).bind(now - (2 * 60 * 1000)).run();
}
