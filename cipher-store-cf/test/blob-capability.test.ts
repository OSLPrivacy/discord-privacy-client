import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { handleDelete, handleFetch, handleUpload } from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

const id = "1".repeat(32);
const fetchCap = "2".repeat(32);
const ackCap = "3".repeat(32);
const manageCap = "4".repeat(32);

async function upload() {
  return handleUpload(new Request("https://cipher.test/v1/blob", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": "3600", "x-osl-blob-id": id,
      "x-osl-fetch-digest": await sha256Hex(fetchCap),
      "x-osl-ack-digest": await sha256Hex(ackCap),
      "x-osl-manage-digest": await sha256Hex(manageCap),
      "x-osl-object-class": "single-ack", "x-osl-delivery-tag": "5".repeat(32),
    }, body: new Uint8Array([7, 8, 9]),
  }), workerEnv());
}

describe("R2 capability blob route", () => {
  it("stores payload bytes only in R2 and burns only with manage_cap", async () => {
    expect((await upload()).status).toBe(201);
    const objectKey = await sha256Hex(fetchCap);
    await expect(env.PAYLOADS.head(objectKey)).resolves.toMatchObject({ size: 3 });
    const columns = await env.DB.prepare("PRAGMA table_info(blobs)").all<{ name: string }>();
    expect(columns.results?.map((column) => column.name)).not.toContain("data");

    expect((await handleFetch(new Request("https://cipher.test/v1/blob/" + id, { headers: { "x-osl-fetch-cap": fetchCap } }), workerEnv(), id)).status).toBe(200);
    expect((await handleDelete(new Request("https://cipher.test/v1/blob/" + id, { method: "DELETE", headers: { "x-osl-fetch-cap": fetchCap } }), workerEnv(), id)).status).toBe(204);
    expect((await handleFetch(new Request("https://cipher.test/v1/blob/" + id, { headers: { "x-osl-fetch-cap": fetchCap } }), workerEnv(), id)).status).toBe(200);

    expect((await handleDelete(new Request("https://cipher.test/v1/blob/" + id, { method: "DELETE", headers: { "x-osl-manage-cap": manageCap } }), workerEnv(), id)).status).toBe(204);
    await expect(env.PAYLOADS.head(objectKey)).resolves.toBeNull();
    expect((await handleFetch(new Request("https://cipher.test/v1/blob/" + id, { headers: { "x-osl-fetch-cap": fetchCap } }), workerEnv(), id)).status).toBe(404);
  });

  it("makes every GET failure the same 404", async () => {
    expect((await upload()).status).toBe(201);
    const missing = await handleFetch(new Request("https://cipher.test/v1/blob/" + id), workerEnv(), id);
    const wrong = await handleFetch(new Request("https://cipher.test/v1/blob/" + id, { headers: { "x-osl-fetch-cap": "f".repeat(32) } }), workerEnv(), id);
    const absent = await handleFetch(new Request("https://cipher.test/v1/blob/" + "a".repeat(32), { headers: { "x-osl-fetch-cap": fetchCap } }), workerEnv(), "a".repeat(32));
    expect([missing.status, wrong.status, absent.status]).toEqual([404, 404, 404]);
    const missingBody = await missing.text();
    const wrongBody = await wrong.text();
    expect(missingBody).toBe(wrongBody);
    expect(wrongBody).toBe(await absent.text());
  });
});
