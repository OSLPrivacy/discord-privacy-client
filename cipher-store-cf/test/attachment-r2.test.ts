import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleAttachmentComplete,
  handleAttachmentDelete,
  handleAttachmentFetch,
  handleAttachmentPartUpload,
  handleAttachmentSessionCreate,
  handleAttachmentUpload,
  MAX_ATTACHMENT_PART_BYTES,
  MAX_DIRECT_ATTACHMENT_BYTES,
} from "../src/endpoints/attachment.js";

interface Row {
  object_key: string;
  size_bytes: number;
  expires_at: number;
  fetch_token_sha256_hex: string;
  state: "uploading" | "completing" | "ready";
  upload_id: string | null;
}

function testEnv(options: { insertChanges?: number } = {}) {
  const rows = new Map<string, Row>();
  const objects = new Map<string, Uint8Array>();
  const put = vi.fn(async (key: string, value: ReadableStream) => {
    const bytes = new Uint8Array(await new Response(value).arrayBuffer());
    objects.set(key, bytes);
    return { key, size: bytes.byteLength } as R2Object;
  });
  const get = vi.fn(async (key: string) => {
    const bytes = objects.get(key);
    if (!bytes) return null;
    return {
      key,
      size: bytes.byteLength,
      body: new Response(bytes).body!,
    } as R2ObjectBody;
  });
  const head = vi.fn(async (key: string) => {
    const bytes = objects.get(key);
    return bytes ? { key, size: bytes.byteLength } as R2Object : null;
  });
  const remove = vi.fn(async (key: string | string[]) => {
    for (const item of Array.isArray(key) ? key : [key]) objects.delete(item);
  });
  const prepare = vi.fn((sql: string) => ({
    bind: (...values: unknown[]) => ({
      run: async () => {
        if (sql.startsWith("INSERT INTO attachment_objects")) {
          if (options.insertChanges !== 0) {
            const [id, objectKey, size, expiresAt, _createdAt, tokenDigest, state, uploadId] = values;
            rows.set(String(id), {
              object_key: String(objectKey),
              size_bytes: Number(size),
              expires_at: Number(expiresAt),
              fetch_token_sha256_hex: String(tokenDigest),
              state: state as Row["state"],
              upload_id: uploadId === null ? null : String(uploadId),
            });
          }
        } else if (sql.startsWith("DELETE FROM attachment_objects")) {
          rows.delete(String(values[0]));
        }
        return { success: true, meta: { changes: options.insertChanges ?? 1 } };
      },
      first: async <T>() => (rows.get(String(values[0])) ?? null) as T | null,
    }),
  }));
  const env = {
    DB: { prepare },
    ATTACHMENTS: { put, get, head, delete: remove },
  } as unknown as Env;
  return { env, rows, objects, put, get, remove };
}

const token = "0123456789abcdef0123456789abcdef";

function uploadRequest(body: BodyInit, contentLength?: number): Request {
  const headers: Record<string, string> = {
    "x-osl-ttl-seconds": "3600",
    "x-osl-fetch-token": token,
  };
  if (contentLength !== undefined) headers["content-length"] = String(contentLength);
  return new Request("https://cipher.test/v1/attachment", {
    method: "POST",
    headers,
    body,
    // Node's fetch implementation requires this for a streamed request body;
    // Workers does not expose or need the option.
    duplex: "half",
  } as RequestInit & { duplex: "half" });
}

describe("R2 attachment transport", () => {
  it("streams upload and fetch while D1 stores only opaque transport metadata", async () => {
    const state = testEnv();
    const bytes = new Uint8Array([1, 2, 3, 4]);
    const uploaded = await handleAttachmentUpload(
      uploadRequest(bytes, bytes.byteLength),
      state.env,
    );
    expect(uploaded.status).toBe(201);
    const result = await uploaded.json() as { id: string; size_bytes: number };
    expect(result.id).toMatch(/^[0-9a-f]{32}$/);
    expect(result.size_bytes).toBe(bytes.byteLength);
    expect(state.put).toHaveBeenCalledTimes(1);
    expect(JSON.stringify([...state.rows.values()])).not.toContain("filename");
    expect(JSON.stringify([...state.rows.values()])).not.toContain(token);
    expect([...state.rows.values()][0]?.fetch_token_sha256_hex).toMatch(/^[0-9a-f]{64}$/);

    const fetched = await handleAttachmentFetch(
      new Request(`https://cipher.test/v1/attachment/${result.id}`, {
        headers: { "x-osl-fetch-token": token },
      }),
      state.env,
      result.id,
    );
    expect(fetched.status).toBe(200);
    expect(new Uint8Array(await fetched.arrayBuffer())).toEqual(bytes);
  });

  it("rejects oversized chunked bodies authoritatively and removes partial R2 state", async () => {
    const state = testEnv();
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array(MAX_DIRECT_ATTACHMENT_BYTES));
        controller.enqueue(new Uint8Array([1]));
        controller.close();
      },
    });
    const response = await handleAttachmentUpload(uploadRequest(stream), state.env);
    expect(response.status).toBe(413);
    expect(state.objects.size).toBe(0);
    expect(state.rows.size).toBe(0);
  });

  it("does not reveal or delete an object with a mismatched capability", async () => {
    const state = testEnv();
    const uploaded = await handleAttachmentUpload(uploadRequest(new Uint8Array([9])), state.env);
    const { id } = await uploaded.json() as { id: string };
    const wrong = "ffffffffffffffffffffffffffffffff";
    const fetchResponse = await handleAttachmentFetch(
      new Request(`https://cipher.test/v1/attachment/${id}`, {
        headers: { "x-osl-fetch-token": wrong },
      }),
      state.env,
      id,
    );
    expect(fetchResponse.status).toBe(403);
    const deleteResponse = await handleAttachmentDelete(
      new Request(`https://cipher.test/v1/attachment/${id}`, {
        method: "DELETE",
        headers: { "x-osl-fetch-token": wrong },
      }),
      state.env,
      id,
    );
    expect(deleteResponse.status).toBe(403);
    expect(state.objects.size).toBe(1);
  });

  it("deletes the R2 object before its metadata", async () => {
    const state = testEnv();
    const uploaded = await handleAttachmentUpload(uploadRequest(new Uint8Array([7])), state.env);
    const { id } = await uploaded.json() as { id: string };
    const response = await handleAttachmentDelete(
      new Request(`https://cipher.test/v1/attachment/${id}`, {
        method: "DELETE",
        headers: { "x-osl-fetch-token": token },
      }),
      state.env,
      id,
    );
    expect(response.status).toBe(204);
    expect(state.objects.size).toBe(0);
    expect(state.rows.size).toBe(0);
  });

  it("removes the R2 object and returns a bounded failure when D1 quota rejects the insert", async () => {
    const state = testEnv({ insertChanges: 0 });
    const response = await handleAttachmentUpload(
      uploadRequest(new Uint8Array([4, 5, 6])),
      state.env,
    );
    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({ error: "storage_capacity" });
    expect(state.objects.size).toBe(0);
    expect(state.rows.size).toBe(0);
  });

  it("assembles a bounded multipart session and exposes it only after completion", async () => {
    const parts = new Map<number, { bytes: Uint8Array; etag: string }>();
    const rows = new Map<string, Row>();
    const objects = new Map<string, Uint8Array>();
    let currentId = "";
    const statements: Array<{ run: () => Promise<unknown> }> = [];
    const prepare = vi.fn((rawSql: string) => {
      const sql = rawSql.replace(/\s+/g, " ").trim();
      return {
        bind: (...values: unknown[]) => {
          const statement = {
            first: async <T>() => (rows.get(String(values[0])) ?? null) as T | null,
            all: async <T>() => ({
              results: [...parts.entries()].sort(([a], [b]) => a - b).map(([part_number, part]) => ({
                part_number,
                size_bytes: part.bytes.byteLength,
                etag: part.etag,
              })) as T[],
            }),
            run: async () => {
              let changes = 0;
              if (sql.startsWith("INSERT INTO attachment_objects")) {
                const [id, objectKey, size, expiresAt, _createdAt, digest, state, uploadId] = values;
                currentId = String(id);
                rows.set(currentId, {
                  object_key: String(objectKey),
                  size_bytes: Number(size),
                  expires_at: Number(expiresAt),
                  fetch_token_sha256_hex: String(digest),
                  state: state as Row["state"],
                  upload_id: String(uploadId),
                });
                changes = 1;
              } else if (sql.startsWith("INSERT INTO attachment_parts")) {
                changes = 1;
              } else if (sql.includes("SET state = 'completing'")) {
                const row = rows.get(String(values[0]));
                if (row?.state === "uploading") { row.state = "completing"; changes = 1; }
              } else if (sql.includes("SET state = 'ready'")) {
                const row = rows.get(String(values[0]));
                if (row?.state === "completing") {
                  row.state = "ready";
                  row.upload_id = null;
                  changes = 1;
                }
              } else if (sql.startsWith("DELETE FROM attachment_parts")) {
                parts.clear();
                changes = 1;
              }
              return { success: true, meta: { changes } };
            },
          };
          statements.push(statement);
          return statement;
        },
      };
    });
    const multipart = {
      uploadId: "opaque-upload-id",
      uploadPart: vi.fn(async (partNumber: number, body: ReadableStream) => {
        const bytes = new Uint8Array(await new Response(body).arrayBuffer());
        const etag = `etag-${partNumber}`;
        parts.set(partNumber, { bytes, etag });
        return { partNumber, etag };
      }),
      complete: vi.fn(async (receipts: R2UploadedPart[]) => {
        const size = receipts.reduce((sum, receipt) => sum + parts.get(receipt.partNumber)!.bytes.byteLength, 0);
        const combined = new Uint8Array(size);
        let offset = 0;
        for (const receipt of receipts) {
          const bytes = parts.get(receipt.partNumber)!.bytes;
          combined.set(bytes, offset);
          offset += bytes.byteLength;
        }
        objects.set(`attachments/${currentId}`, combined);
        return { key: `attachments/${currentId}`, size } as R2Object;
      }),
      abort: vi.fn(async () => undefined),
    };
    const env = {
      DB: {
        prepare,
        batch: async (batch: Array<{ run: () => Promise<unknown> }>) => Promise.all(batch.map((item) => item.run())),
      },
      ATTACHMENTS: {
        createMultipartUpload: vi.fn(async () => multipart),
        resumeMultipartUpload: vi.fn(() => multipart),
        get: vi.fn(async (key: string) => {
          const bytes = objects.get(key);
          return bytes ? { key, size: bytes.byteLength, body: new Response(bytes).body! } as R2ObjectBody : null;
        }),
        head: vi.fn(async (key: string) => {
          const bytes = objects.get(key);
          return bytes ? { key, size: bytes.byteLength } as R2Object : null;
        }),
      },
    } as unknown as Env;

    const session = await handleAttachmentSessionCreate(new Request("https://cipher.test/v1/attachment/session", {
      method: "POST",
      headers: {
        "x-osl-ttl-seconds": "3600",
        "x-osl-fetch-token": token,
        "x-osl-size-bytes": String(MAX_ATTACHMENT_PART_BYTES + 2),
      },
    }), env);
    expect(session.status).toBe(201);
    const { id } = await session.json() as { id: string };
    expect(JSON.stringify([...rows.values()])).not.toContain(token);

    const beforeComplete = await handleAttachmentFetch(new Request(`https://cipher.test/v1/attachment/${id}`, {
      headers: { "x-osl-fetch-token": token },
    }), env, id);
    expect(beforeComplete.status).toBe(404);

    const firstPart = new Uint8Array(MAX_ATTACHMENT_PART_BYTES);
    firstPart[0] = 1;
    firstPart[firstPart.length - 1] = 3;
    for (const [partNumber, bytes] of [[1, firstPart], [2, new Uint8Array([4, 5])]] as const) {
      const response = await handleAttachmentPartUpload(new Request(
        `https://cipher.test/v1/attachment/${id}/part/${partNumber}`,
        {
          method: "PUT",
          headers: { "x-osl-fetch-token": token, "content-length": String(bytes.byteLength) },
          body: bytes,
          duplex: "half",
        } as RequestInit & { duplex: "half" },
      ), env, id, partNumber);
      expect(response.status).toBe(201);
    }

    const completed = await handleAttachmentComplete(new Request(
      `https://cipher.test/v1/attachment/${id}/complete`,
      { method: "POST", headers: { "x-osl-fetch-token": token } },
    ), env, id);
    expect(completed.status).toBe(201);
    const fetched = await handleAttachmentFetch(new Request(`https://cipher.test/v1/attachment/${id}`, {
      headers: { "x-osl-fetch-token": token },
    }), env, id);
    const fetchedBytes = new Uint8Array(await fetched.arrayBuffer());
    expect(fetchedBytes).toHaveLength(MAX_ATTACHMENT_PART_BYTES + 2);
    expect(fetchedBytes[0]).toBe(1);
    expect(fetchedBytes[MAX_ATTACHMENT_PART_BYTES - 1]).toBe(3);
    expect(fetchedBytes.slice(-2)).toEqual(new Uint8Array([4, 5]));
  });
});
