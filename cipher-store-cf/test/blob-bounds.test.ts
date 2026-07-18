import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleUpload,
  MAX_BLOB_BYTES,
  readBoundedBody,
} from "../src/endpoints/blob.js";

describe("cipher upload body bounds", () => {
  it("accepts exactly the maximum streamed byte count", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      body: new Uint8Array(MAX_BLOB_BYTES),
    });
    const result = await readBoundedBody(request, MAX_BLOB_BYTES);
    expect(result.status).toBe("ok");
    if (result.status === "ok") {
      expect(result.bytes.byteLength).toBe(MAX_BLOB_BYTES);
    }
  });

  it("stops a streamed body as soon as it exceeds the cap", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      body: new Uint8Array(MAX_BLOB_BYTES + 1),
    });
    await expect(readBoundedBody(request, MAX_BLOB_BYTES)).resolves.toEqual({
      status: "too_large",
    });
  });

  it("rejects an oversized Content-Length before touching storage", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      headers: { "content-length": String(MAX_BLOB_BYTES + 1) },
      body: new Uint8Array([1]),
    });
    const env = {
      get DB(): never {
        throw new Error("storage must not be touched");
      },
    } as unknown as Env;
    const response = await handleUpload(request, env);
    expect(response.status).toBe(413);
    expect(await response.json()).toEqual({
      error: "too_large",
      message: `blob exceeds ${MAX_BLOB_BYTES} bytes`,
    });
  });

  it("rejects a malformed Content-Length before reading the body", async () => {
    const request = new Request("https://cipher.test/v1/blob", {
      method: "POST",
      headers: { "content-length": "1junk" },
      body: new Uint8Array([1]),
    });
    const response = await handleUpload(request, {} as Env);
    expect(response.status).toBe(400);
  });
});
