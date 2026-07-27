import { describe, expect, it } from "vitest";
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
import {
  MAX_LIVE_ATTACHMENT_BYTES,
  MAX_SEALED_ATTACHMENT_BYTES,
} from "../src/lib/attachment-limits.js";
import {
  d1Count,
  d1First,
  d1Run,
  workerEnv,
} from "./helpers/workerd.js";

interface Row {
  object_key: string;
  size_bytes: number;
  expires_at: number;
  content_expires_at: number | null;
  fetch_token_sha256_hex: string;
  state: "uploading" | "completing" | "ready";
  upload_id: string | null;
}

async function attachmentRow(id: string): Promise<Row> {
  return d1First<Row>(
    `SELECT object_key, size_bytes, expires_at, content_expires_at,
            fetch_token_sha256_hex, state, upload_id
       FROM attachment_objects WHERE id = ? LIMIT 1`,
    id,
  );
}

async function insertCapacityRows(): Promise<void> {
  const now = Math.floor(Date.now() / 1000);
  const rowSize = MAX_SEALED_ATTACHMENT_BYTES - 1024 * 1024;
  const rows = Math.ceil(MAX_LIVE_ATTACHMENT_BYTES / rowSize);
  for (let index = 0; index < rows; index++) {
    await d1Run(
      `INSERT INTO attachment_objects
       (id, object_key, size_bytes, expires_at, content_expires_at, created_at,
        fetch_token_sha256_hex, state, upload_id)
       VALUES (?, ?, ?, ?, ?, ?, ?, 'ready', NULL)`,
      index.toString(16).padStart(32, "0"),
      `attachments/capacity-${index}`,
      rowSize,
      now + 3600,
      now + 3600,
      now,
      "a".repeat(64),
    );
  }
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
    const env = workerEnv();
    const bytes = new Uint8Array([1, 2, 3, 4]);
    const uploaded = await handleAttachmentUpload(
      uploadRequest(bytes, bytes.byteLength),
      env,
    );
    expect(uploaded.status).toBe(201);
    const result = await uploaded.json() as { id: string; size_bytes: number };
    expect(result.id).toMatch(/^[0-9a-f]{32}$/);
    expect(result.size_bytes).toBe(bytes.byteLength);
    const row = await attachmentRow(result.id);
    expect(row.object_key).toBe(`attachments/${result.id}`);
    expect(row.size_bytes).toBe(bytes.byteLength);
    expect(row.state).toBe("ready");
    expect(JSON.stringify(row)).not.toContain("filename");
    expect(JSON.stringify(row)).not.toContain(token);
    expect(row.fetch_token_sha256_hex).toMatch(/^[0-9a-f]{64}$/);
    expect(await env.ATTACHMENTS.head(row.object_key)).toMatchObject({ size: bytes.byteLength });

    const fetched = await handleAttachmentFetch(
      new Request(`https://cipher.test/v1/attachment/${result.id}`, {
        headers: { "x-osl-fetch-token": token },
      }),
      env,
      result.id,
    );
    expect(fetched.status).toBe(200);
    expect(new Uint8Array(await fetched.arrayBuffer())).toEqual(bytes);
  });

  it("hands R2 a known-length body, never a transformed stream", async () => {
    // Regression guard for a defect that shipped green: the upload path used to
    // pipe the request body through a counting TransformStream and hand the
    // result to R2. workerd requires a streamed put/uploadPart body to have a
    // known length and rejects a piped stream with "Provided readable stream
    // must have a known length", so every attachment upload 500'd on the real
    // runtime. This now runs against real R2 in workerd: if the source hands R2
    // the transformed stream instead of buffered bytes, this request fails.
    const env = workerEnv();
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array([1, 2, 3]));
        controller.enqueue(new Uint8Array([4, 5]));
        controller.close();
      },
    }).pipeThrough(new TransformStream<Uint8Array, Uint8Array>());
    const uploaded = await handleAttachmentUpload(uploadRequest(stream), env);
    expect(uploaded.status).toBe(201);
    const result = await uploaded.json() as { id: string; size_bytes: number };
    const row = await attachmentRow(result.id);
    expect(result.size_bytes).toBe(5);
    expect(await env.ATTACHMENTS.head(row.object_key)).toMatchObject({ size: 5 });
  });

  it("rejects oversized chunked bodies authoritatively and removes partial R2 state", async () => {
    const real = workerEnv();
    const touchedKeys: string[] = [];
    const env = workerEnv({
      ATTACHMENTS: {
        put: async (key: string, value: unknown, options?: R2PutOptions) => {
          touchedKeys.push(key);
          return real.ATTACHMENTS.put(key, value as never, options);
        },
        get: real.ATTACHMENTS.get.bind(real.ATTACHMENTS),
        head: real.ATTACHMENTS.head.bind(real.ATTACHMENTS),
        delete: real.ATTACHMENTS.delete.bind(real.ATTACHMENTS),
        createMultipartUpload: real.ATTACHMENTS.createMultipartUpload.bind(real.ATTACHMENTS),
        resumeMultipartUpload: real.ATTACHMENTS.resumeMultipartUpload.bind(real.ATTACHMENTS),
      } as unknown as R2Bucket,
    });
    const stream = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(new Uint8Array(MAX_DIRECT_ATTACHMENT_BYTES));
        controller.enqueue(new Uint8Array([1]));
        controller.close();
      },
    });
    const response = await handleAttachmentUpload(uploadRequest(stream), env);
    expect(response.status).toBe(413);
    expect(touchedKeys).toHaveLength(0);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
  });

  it("does not reveal or delete an object with a mismatched capability", async () => {
    const env = workerEnv();
    const uploaded = await handleAttachmentUpload(uploadRequest(new Uint8Array([9])), env);
    const { id } = await uploaded.json() as { id: string };
    const row = await attachmentRow(id);
    const wrong = "ffffffffffffffffffffffffffffffff";
    const fetchResponse = await handleAttachmentFetch(
      new Request(`https://cipher.test/v1/attachment/${id}`, {
        headers: { "x-osl-fetch-token": wrong },
      }),
      env,
      id,
    );
    expect(fetchResponse.status).toBe(403);
    const deleteResponse = await handleAttachmentDelete(
      new Request(`https://cipher.test/v1/attachment/${id}`, {
        method: "DELETE",
        headers: { "x-osl-fetch-token": wrong },
      }),
      env,
      id,
    );
    expect(deleteResponse.status).toBe(403);
    expect(await env.ATTACHMENTS.head(row.object_key)).not.toBeNull();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?", id)).toBe(1);
  });

  it("deletes the R2 object before its metadata", async () => {
    const env = workerEnv();
    const uploaded = await handleAttachmentUpload(uploadRequest(new Uint8Array([7])), env);
    const { id } = await uploaded.json() as { id: string };
    const row = await attachmentRow(id);
    const response = await handleAttachmentDelete(
      new Request(`https://cipher.test/v1/attachment/${id}`, {
        method: "DELETE",
        headers: { "x-osl-fetch-token": token },
      }),
      env,
      id,
    );
    expect(response.status).toBe(204);
    expect(await env.ATTACHMENTS.head(row.object_key)).toBeNull();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?", id)).toBe(0);
  });

  it("removes the R2 object and returns a bounded failure when D1 quota rejects the insert", async () => {
    await insertCapacityRows();
    const real = workerEnv();
    const deletedKeys: string[] = [];
    const env = workerEnv({
      ATTACHMENTS: {
        put: real.ATTACHMENTS.put.bind(real.ATTACHMENTS),
        get: real.ATTACHMENTS.get.bind(real.ATTACHMENTS),
        head: real.ATTACHMENTS.head.bind(real.ATTACHMENTS),
        delete: async (key: string | string[]) => {
          deletedKeys.push(...(Array.isArray(key) ? key : [key]));
          return real.ATTACHMENTS.delete(key);
        },
        createMultipartUpload: real.ATTACHMENTS.createMultipartUpload.bind(real.ATTACHMENTS),
        resumeMultipartUpload: real.ATTACHMENTS.resumeMultipartUpload.bind(real.ATTACHMENTS),
      } as unknown as R2Bucket,
    });
    const response = await handleAttachmentUpload(
      uploadRequest(new Uint8Array([4, 5, 6])),
      env,
    );
    expect(response.status).toBe(503);
    expect(await response.json()).toMatchObject({ error: "storage_capacity" });
    expect(deletedKeys).toHaveLength(1);
    expect(await real.ATTACHMENTS.head(deletedKeys[0]!)).toBeNull();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBeGreaterThan(0);
  });

  it("assembles a bounded multipart session and exposes it only after completion", async () => {
    const env = workerEnv();

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
    expect(JSON.stringify(await attachmentRow(id))).not.toContain(token);

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
    const row = await attachmentRow(id);
    expect(row.state).toBe("ready");
    expect(row.upload_id).toBeNull();
    expect(await env.ATTACHMENTS.head(row.object_key)).toMatchObject({ size: MAX_ATTACHMENT_PART_BYTES + 2 });
    const fetched = await handleAttachmentFetch(new Request(`https://cipher.test/v1/attachment/${id}`, {
      headers: { "x-osl-fetch-token": token },
    }), env, id);
    expect(fetched.status).toBe(200);
    const fetchedBytes = new Uint8Array(await fetched.arrayBuffer());
    expect(fetchedBytes).toHaveLength(MAX_ATTACHMENT_PART_BYTES + 2);
    expect(fetchedBytes[0]).toBe(1);
    expect(fetchedBytes[MAX_ATTACHMENT_PART_BYTES - 1]).toBe(3);
    expect(fetchedBytes.slice(-2)).toEqual(new Uint8Array([4, 5]));
  });
});
