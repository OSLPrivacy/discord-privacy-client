import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleDelete,
  handleFetch,
  handleUpload,
} from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

const FETCH_TOKEN = "0123456789abcdef0123456789abcdef";
const MANAGE_TOKEN = "fedcba9876543210fedcba9876543210";

async function uploadRequest(): Promise<Request> {
  return new Request("https://cipher.test/v1/blob", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-expiry-mode": "absolute",
      "x-osl-blob-id": "1".repeat(32),
      "x-osl-fetch-digest": await sha256Hex(FETCH_TOKEN),
      "x-osl-ack-digest": await sha256Hex("a".repeat(32)),
      "x-osl-manage-digest": await sha256Hex(MANAGE_TOKEN),
      "x-osl-object-class": "single-ack",
      "x-osl-delivery-tag": "5".repeat(32),
    },
    body: new Uint8Array([1]),
  });
}

describe("blob manage capability", () => {
  it("does not let a capability bearer burn a blob", async () => {
    const env = workerEnv();
    const uploaded = await handleUpload(await uploadRequest(), env);
    expect(uploaded.status).toBe(201);
    const { id } = await uploaded.json() as { id: string };
    const url = `https://cipher.test/v1/blob/${id}`;

    const fetchBearerDelete = await handleDelete(
      new Request(url, { method: "DELETE", headers: { "x-osl-fetch-cap": FETCH_TOKEN } }),
      env,
      id,
    );
    // Burn is oracle-free: an unauthorized request gets the same idempotent
    // response as a repeated successful burn, but cannot alter storage.
    expect(fetchBearerDelete.status).toBe(204);

    const remainsFetchable = await handleFetch(
      new Request(url, { headers: { "x-osl-fetch-cap": FETCH_TOKEN } }),
      env,
      id,
    );
    expect(remainsFetchable.status).toBe(200);

    const managedDelete = await handleDelete(
      new Request(url, { method: "DELETE", headers: { "x-osl-manage-cap": MANAGE_TOKEN } }),
      env,
      id,
    );
    expect(managedDelete.status).toBe(204);
    expect((await handleFetch(
      new Request(url, { headers: { "x-osl-fetch-cap": FETCH_TOKEN } }),
      env,
      id,
    )).status).toBe(404);
  });

  it("rejects an upload that omits its manage capability", async () => {
    const request = await uploadRequest();
    request.headers.delete("x-osl-manage-digest");
    const response = await handleUpload(request, workerEnv() as Env);
    expect(response.status).toBe(400);
  });
});
