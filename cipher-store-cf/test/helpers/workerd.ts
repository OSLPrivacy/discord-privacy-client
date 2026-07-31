import { env } from "cloudflare:test";
import type { Env } from "../../src/env.js";

type Bound = string | number | bigint | null | Uint8Array | ArrayBuffer;

export function workerEnv(overrides: Partial<Env> = {}): Env {
  return {
    DB: env.DB,
    ATTACHMENTS: env.ATTACHMENTS,
    RATE_LIMIT: env.RATE_LIMIT,
    RATE_LIMIT_HASH_KEY: env.RATE_LIMIT_HASH_KEY,
    ...overrides,
  } as Env;
}

export async function d1Run(sql: string, ...values: Bound[]): Promise<D1Result> {
  return env.DB.prepare(sql).bind(...values).run();
}

/// Run one statement over many bind sets in D1 batches instead of one
/// round trip per row. Seeding a few hundred rows one `await` at a time is
/// the dominant cost in the quota/sweep fixtures.
export async function d1BatchRun(
  sql: string,
  rows: Bound[][],
  chunkSize = 64,
): Promise<void> {
  const statement = env.DB.prepare(sql);
  for (let start = 0; start < rows.length; start += chunkSize) {
    await env.DB.batch(
      rows.slice(start, start + chunkSize).map((values) => statement.bind(...values)),
    );
  }
}

export async function d1First<T>(sql: string, ...values: Bound[]): Promise<T> {
  const row = await env.DB.prepare(sql).bind(...values).first<T>();
  if (!row) throw new Error(`missing D1 row for query: ${sql}`);
  return row;
}

export async function d1All<T>(sql: string, ...values: Bound[]): Promise<T[]> {
  const result = await env.DB.prepare(sql).bind(...values).all<T>();
  return result.results ?? [];
}

export async function d1Count(sql: string, ...values: Bound[]): Promise<number> {
  const row = await env.DB.prepare(sql).bind(...values).first<Record<string, unknown>>();
  return Number(Object.values(row ?? { c: 0 })[0] ?? 0);
}

export function blobBytes(value: unknown): Uint8Array {
  if (value instanceof Uint8Array) return value;
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (ArrayBuffer.isView(value)) {
    const view = value as ArrayBufferView;
    return new Uint8Array(view.buffer, view.byteOffset, view.byteLength);
  }
  if (Array.isArray(value)) return new Uint8Array(value as number[]);
  if (typeof value === "string") return new TextEncoder().encode(value);
  return new Uint8Array();
}
