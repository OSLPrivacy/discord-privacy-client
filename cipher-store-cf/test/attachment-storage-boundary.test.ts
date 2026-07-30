import { describe, expect, it, vi } from "vitest";
import type { Env } from "../src/env.js";
import worker from "../src/index.js";
import {
  d1Count,
  d1First,
  d1Run,
  workerEnv,
} from "./helpers/workerd.js";

const TOKEN = "0123456789abcdef0123456789abcdef";
const CTX = {} as ExecutionContext;

interface AttachmentRow {
  object_key: string;
  state: "uploading" | "completing" | "ready";
  upload_id: string | null;
}

function boundBucket(
  real: R2Bucket,
  overrides: Partial<Record<keyof R2Bucket, unknown>>,
): R2Bucket {
  return new Proxy(real, {
    get(target, property, receiver) {
      if (Object.hasOwn(overrides, property)) {
        return overrides[property as keyof R2Bucket];
      }
      const value = Reflect.get(target, property, receiver);
      return typeof value === "function" ? value.bind(target) : value;
    },
  });
}

function request(
  path: string,
  method: string,
  body?: Uint8Array,
  headers: Record<string, string> = {},
): Request {
  return new Request(`https://cipher.test${path}`, {
    method,
    headers: {
      "cf-connecting-ip": "192.0.2.30",
      ...headers,
      ...(body ? { "content-length": String(body.byteLength) } : {}),
    },
    body,
    duplex: "half",
  } as RequestInit & { duplex: "half" });
}

function directUpload(bytes: Uint8Array): Request {
  return request("/v1/attachment", "POST", bytes, {
    "x-osl-ttl-seconds": "3600",
    "x-osl-fetch-token": TOKEN,
  });
}

async function uploadDirect(
  env: Env,
  bytes: Uint8Array,
): Promise<{ response: Response; id: string; row: AttachmentRow }> {
  const response = await worker.fetch(directUpload(bytes), env, CTX);
  const body = await response.clone().json() as { id?: string };
  const id = body.id ?? "";
  const row = id
    ? await d1First<AttachmentRow>(
      "SELECT object_key, state, upload_id FROM attachment_objects WHERE id = ?",
      id,
    )
    : { object_key: "", state: "uploading", upload_id: null } as AttachmentRow;
  return { response, id, row };
}

async function createMultipart(
  env: Env,
  bytes: Uint8Array,
): Promise<{ id: string; row: AttachmentRow }> {
  const session = await worker.fetch(request(
    "/v1/attachment/session",
    "POST",
    undefined,
    {
      "x-osl-ttl-seconds": "3600",
      "x-osl-fetch-token": TOKEN,
      "x-osl-size-bytes": String(bytes.byteLength),
    },
  ), env, CTX);
  expect(session.status).toBe(201);
  const { id } = await session.json() as { id: string };
  const uploaded = await worker.fetch(request(
    `/v1/attachment/${id}/part/1`,
    "PUT",
    bytes,
    { "x-osl-fetch-token": TOKEN },
  ), env, CTX);
  expect(uploaded.status).toBe(201);
  return {
    id,
    row: await d1First<AttachmentRow>(
      "SELECT object_key, state, upload_id FROM attachment_objects WHERE id = ?",
      id,
    ),
  };
}

describe("registered attachment storage boundary", () => {
  it("routes nonempty direct bytes through real R2 and reads them back exactly", async () => {
    const env = workerEnv();
    const bytes = new Uint8Array([3, 1, 4, 1, 5, 9]);
    const uploaded = await uploadDirect(env, bytes);
    expect(uploaded.response.status).toBe(201);
    expect(uploaded.row.state).toBe("ready");

    const fetched = await worker.fetch(request(
      `/v1/attachment/${uploaded.id}`,
      "GET",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(fetched.status).toBe(200);
    expect(new Uint8Array(await fetched.arrayBuffer())).toEqual(bytes);
  });

  it("refuses a no-op accept-any-body put receipt and commits no metadata", async () => {
    const real = workerEnv();
    const put = vi.fn(async (key: string, value: unknown) => ({
      key,
      version: "fake-version",
      size: value instanceof Uint8Array ? value.byteLength : 6,
      etag: "fake-etag",
      httpEtag: "\"fake-etag\"",
      checksums: { toJSON: () => ({}) },
      uploaded: new Date(),
      storageClass: "Standard",
      writeHttpMetadata() {},
    } as unknown as R2Object));
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { put }),
    });
    const response = await worker.fetch(
      directUpload(new Uint8Array([1, 2, 3])),
      env,
      CTX,
    );
    expect(response.status).toBe(500);
    expect(put).toHaveBeenCalledOnce();
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
  });

  it("refuses and removes a same-size wrong object written by R2", async () => {
    const real = workerEnv();
    const put = vi.fn((
      key: string,
      value: unknown,
      options?: R2PutOptions,
    ) => {
      const length = value instanceof Uint8Array ? value.byteLength : 3;
      return real.ATTACHMENTS.put(key, new Uint8Array(length).fill(0xa5), {
        onlyIf: options?.onlyIf,
      });
    });
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { put }),
    });
    const response = await worker.fetch(
      directUpload(new Uint8Array([7, 8, 9])),
      env,
      CTX,
    );
    expect(response.status).toBe(500);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    const keys = (await real.ATTACHMENTS.list()).objects.map((object) => object.key);
    expect(keys.filter((key) => key.startsWith("attachments/"))).toEqual([]);
  });

  it("removes a partial object after a put exception and emits no receipt", async () => {
    const real = workerEnv();
    const put = vi.fn(async (
      key: string,
      _value: unknown,
      options?: R2PutOptions,
    ) => {
      await real.ATTACHMENTS.put(key, new Uint8Array([1]), {
        onlyIf: options?.onlyIf,
      });
      throw new Error("injected partial write");
    });
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { put }),
    });
    const response = await worker.fetch(
      directUpload(new Uint8Array([1, 2, 3])),
      env,
      CTX,
    );
    expect(response.status).toBe(500);
    expect(await d1Count("SELECT COUNT(*) AS c FROM attachment_objects")).toBe(0);
    expect((await real.ATTACHMENTS.list()).objects).toEqual([]);
  });

  it("makes a claimed-length truncated R2 body fail during download", async () => {
    const real = workerEnv();
    const bytes = new Uint8Array([8, 6, 7, 5, 3, 0, 9]);
    const uploaded = await uploadDirect(real, bytes);
    expect(uploaded.response.status).toBe(201);
    const get = vi.fn(async (key: string, options?: R2GetOptions) => {
      const object = await real.ATTACHMENTS.get(key, options);
      if (!object || !("body" in object)) return object;
      return new Proxy(object, {
        get(target, property, receiver) {
          if (property === "body") {
            return new ReadableStream<Uint8Array>({
              start(controller) {
                controller.enqueue(bytes.slice(0, -1));
                controller.close();
              },
            });
          }
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { get }),
    });
    const fetched = await worker.fetch(request(
      `/v1/attachment/${uploaded.id}`,
      "GET",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(fetched.status).toBe(200);
    await expect(fetched.arrayBuffer()).rejects.toThrow(/length/);
  });

  it("refuses a same-size object from the wrong R2 key", async () => {
    const real = workerEnv();
    const bytes = new Uint8Array([2, 7, 1, 8]);
    const uploaded = await uploadDirect(real, bytes);
    expect(uploaded.response.status).toBe(201);
    await real.ATTACHMENTS.put("attachments/substituted", bytes);
    const get = vi.fn(() => real.ATTACHMENTS.get("attachments/substituted"));
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { get }),
    });
    const fetched = await worker.fetch(request(
      `/v1/attachment/${uploaded.id}`,
      "GET",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(fetched.status).not.toBe(200);
  });

  it("does not read or delete storage when the bearer token mismatches", async () => {
    const real = workerEnv();
    const bytes = new Uint8Array([1, 2, 1, 2]);
    const uploaded = await uploadDirect(real, bytes);
    expect(uploaded.response.status).toBe(201);
    const wrong = "ffffffffffffffffffffffffffffffff";
    const get = vi.fn(real.ATTACHMENTS.get.bind(real.ATTACHMENTS));
    const remove = vi.fn(real.ATTACHMENTS.delete.bind(real.ATTACHMENTS));
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { get, delete: remove }),
    });
    const fetched = await worker.fetch(request(
      `/v1/attachment/${uploaded.id}`,
      "GET",
      undefined,
      { "x-osl-fetch-token": wrong },
    ), env, CTX);
    const deleted = await worker.fetch(request(
      `/v1/attachment/${uploaded.id}`,
      "DELETE",
      undefined,
      { "x-osl-fetch-token": wrong },
    ), env, CTX);
    expect(fetched.status).toBe(403);
    expect(deleted.status).toBe(403);
    expect(get).not.toHaveBeenCalled();
    expect(remove).not.toHaveBeenCalled();
    expect(await real.ATTACHMENTS.head(uploaded.row.object_key)).not.toBeNull();
    expect(await d1Count(
      "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
      uploaded.id,
    )).toBe(1);
  });

  it("refuses a duplicate multipart finalize instead of minting a replay receipt", async () => {
    const env = workerEnv();
    const { id } = await createMultipart(env, new Uint8Array([4, 2]));
    const complete = () => worker.fetch(request(
      `/v1/attachment/${id}/complete`,
      "POST",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect((await complete()).status).toBe(201);
    const replay = await complete();
    expect(replay.status).toBe(409);
    expect(await replay.json()).toMatchObject({ error: "upload_already_complete" });
  });

  it("refuses a mismatched completed-object receipt and leaves recovery metadata fenced", async () => {
    const real = workerEnv();
    const { id } = await createMultipart(real, new Uint8Array([2, 4, 6]));
    const resumeMultipartUpload = vi.fn((key: string, uploadId: string) => {
      const upload = real.ATTACHMENTS.resumeMultipartUpload(key, uploadId);
      return new Proxy(upload, {
        get(target, property, receiver) {
          if (property === "complete") {
            return async (parts: R2UploadedPart[]) => {
              const completed = await target.complete(parts);
              return new Proxy(completed, {
                get(object, objectProperty, objectReceiver) {
                  if (objectProperty === "key") return "attachments/substituted";
                  const value = Reflect.get(
                    object,
                    objectProperty,
                    objectReceiver,
                  );
                  return typeof value === "function" ? value.bind(object) : value;
                },
              });
            };
          }
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { resumeMultipartUpload }),
    });
    const response = await worker.fetch(request(
      `/v1/attachment/${id}/complete`,
      "POST",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(response.status).toBe(500);
    expect(await d1First<{ state: string }>(
      "SELECT state FROM attachment_objects WHERE id = ?",
      id,
    )).toEqual({ state: "completing" });
  });

  it("refuses a mismatched multipart receipt before persisting its etag", async () => {
    const real = workerEnv();
    const session = await worker.fetch(request(
      "/v1/attachment/session",
      "POST",
      undefined,
      {
        "x-osl-ttl-seconds": "3600",
        "x-osl-fetch-token": TOKEN,
        "x-osl-size-bytes": "2",
      },
    ), real, CTX);
    const { id } = await session.json() as { id: string };
    const row = await d1First<AttachmentRow>(
      "SELECT object_key, state, upload_id FROM attachment_objects WHERE id = ?",
      id,
    );
    const resumeMultipartUpload = vi.fn((key: string, uploadId: string) => {
      const upload = real.ATTACHMENTS.resumeMultipartUpload(key, uploadId);
      return new Proxy(upload, {
        get(target, property, receiver) {
          if (property === "uploadPart") {
            return async (partNumber: number, value: unknown) => {
              const receipt = await target.uploadPart(partNumber, value as never);
              return { ...receipt, partNumber: partNumber + 1 };
            };
          }
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { resumeMultipartUpload }),
    });
    const part = await worker.fetch(request(
      `/v1/attachment/${id}/part/1`,
      "PUT",
      new Uint8Array([1, 2]),
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(part.status).toBe(500);
    expect(await d1Count(
      `SELECT COUNT(*) AS c FROM attachment_parts
        WHERE attachment_id = ? AND etag IS NOT NULL`,
      id,
    )).toBe(0);
    expect(row.upload_id).not.toBeNull();
  });

  it("does not delete metadata or return 204 when R2 deletion is a no-op", async () => {
    const real = workerEnv();
    const bytes = new Uint8Array([1, 6, 1, 8]);
    const uploaded = await uploadDirect(real, bytes);
    expect(uploaded.response.status).toBe(201);
    const remove = vi.fn(async () => undefined);
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { delete: remove }),
    });
    const response = await worker.fetch(request(
      `/v1/attachment/${uploaded.id}`,
      "DELETE",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(response.status).toBe(500);
    expect(remove).toHaveBeenCalled();
    expect(await real.ATTACHMENTS.head(uploaded.row.object_key)).not.toBeNull();
    expect(await d1Count(
      "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
      uploaded.id,
    )).toBe(1);
  });

  it("retains a multipart reservation when invalid-body abort is ambiguous", async () => {
    const real = workerEnv();
    const session = await worker.fetch(request(
      "/v1/attachment/session",
      "POST",
      undefined,
      {
        "x-osl-ttl-seconds": "3600",
        "x-osl-fetch-token": TOKEN,
        "x-osl-size-bytes": "2",
      },
    ), real, CTX);
    expect(session.status).toBe(201);
    const { id } = await session.json() as { id: string };
    const abort = vi.fn(async () => {
      throw new Error("abort outcome unknown");
    });
    const resumeMultipartUpload = vi.fn((key: string, uploadId: string) => {
      const upload = real.ATTACHMENTS.resumeMultipartUpload(key, uploadId);
      return new Proxy(upload, {
        get(target, property, receiver) {
          if (property === "abort") return abort;
          const value = Reflect.get(target, property, receiver);
          return typeof value === "function" ? value.bind(target) : value;
        },
      });
    });
    const env = workerEnv({
      ATTACHMENTS: boundBucket(real.ATTACHMENTS, { resumeMultipartUpload }),
    });
    const truncated = new Request(
      `https://cipher.test/v1/attachment/${id}/part/1`,
      {
        method: "PUT",
        headers: {
          "cf-connecting-ip": "192.0.2.30",
          "x-osl-fetch-token": TOKEN,
          "content-length": "2",
        },
        body: new Uint8Array([1]),
        duplex: "half",
      } as RequestInit & { duplex: "half" },
    );
    const response = await worker.fetch(truncated, env, CTX);
    expect(response.status).toBe(500);
    expect(abort).toHaveBeenCalledOnce();
    expect(await d1Count(
      "SELECT COUNT(*) AS c FROM attachment_objects WHERE id = ?",
      id,
    )).toBe(1);
  });

  it("fails finalize on real R2 when the persisted upload id or etag is wrong", async () => {
    const env = workerEnv();
    const wrongUpload = await createMultipart(env, new Uint8Array([5, 5]));
    await d1Run(
      "UPDATE attachment_objects SET upload_id = ? WHERE object_key = ?",
      "not-the-real-upload",
      wrongUpload.row.object_key,
    );
    const uploadResponse = await worker.fetch(request(
      `/v1/attachment/${wrongUpload.id}/complete`,
      "POST",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(uploadResponse.status).toBe(500);

    const wrongEtag = await createMultipart(env, new Uint8Array([6, 6]));
    await d1Run(
      "UPDATE attachment_parts SET etag = ? WHERE attachment_id = ?",
      "not-the-real-etag",
      wrongEtag.id,
    );
    const etagResponse = await worker.fetch(request(
      `/v1/attachment/${wrongEtag.id}/complete`,
      "POST",
      undefined,
      { "x-osl-fetch-token": TOKEN },
    ), env, CTX);
    expect(etagResponse.status).toBe(500);
  });
});
