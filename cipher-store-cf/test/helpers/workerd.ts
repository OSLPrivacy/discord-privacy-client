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
