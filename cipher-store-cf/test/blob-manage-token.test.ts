import { describe, expect, it } from "vitest";
import type { Env } from "../src/env.js";
import {
  handleDelete,
  handleFetch,
  handleUpload,
} from "../src/endpoints/blob.js";
import { workerEnv } from "./helpers/workerd.js";

const FETCH_TOKEN = "0123456789abcdef0123456789abcdef";
const MANAGE_TOKEN = "fedcba9876543210fedcba9876543210";

function uploadRequest(): Request {
  return new Request("https://cipher.test/v1/blob", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": "3600",
      "x-osl-fetch-token": FETCH_TOKEN,
      "x-osl-manage-token": MANAGE_TOKEN,
    },
    body: new Uint8Array([1, 2, 3]),
  });
}

describe("blob manage capability", () => {
  it("does not let a capability bearer burn a blob", async () => {
    const env = workerEnv();
    const uploaded = await handleUpload(uploadRequest(), env);
    expect(uploaded.status).toBe(201);
    const { id } = await uploaded.json() as { id: string };
    const url = `https://cipher.test/v1/blob/${id}`;

    const fetchBearerDelete = await handleDelete(
      new Request(url, { method: "DELETE", headers: { "x-osl-fetch-token": FETCH_TOKEN } }),
      env,
      id,
    );
    expect(fetchBearerDelete.status).toBe(403);

    const remainsFetchable = await handleFetch(
      new Request(url, { headers: { "x-osl-fetch-token": FETCH_TOKEN } }),
      env,
      id,
    );
    expect(remainsFetchable.status).toBe(200);

    const managedDelete = await handleDelete(
      new Request(url, { method: "DELETE", headers: { "x-osl-manage-token": MANAGE_TOKEN } }),
      env,
      id,
    );
    expect(managedDelete.status).toBe(204);
    expect((await handleFetch(
      new Request(url, { headers: { "x-osl-fetch-token": FETCH_TOKEN } }),
      env,
      id,
    )).status).toBe(404);
  });

  it("rejects an upload that omits its manage capability", async () => {
    const request = uploadRequest();
    request.headers.delete("x-osl-manage-token");
    const response = await handleUpload(request, workerEnv() as Env);
    expect(response.status).toBe(400);
  });
});
