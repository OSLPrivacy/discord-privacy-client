import { describe, expect, it } from "vitest";
import { handleUpload } from "../src/endpoints/blob.js";
import { sha256Hex } from "../src/lib/digest.js";
import { workerEnv } from "./helpers/workerd.js";

async function request(ttl: string, mode?: "absolute"): Promise<Request> {
  const seed = `${ttl}-${mode ?? "default"}`;
  const digest = await sha256Hex(seed);
  return new Request("https://cipher.test/v1/blob", {
    method: "POST",
    headers: {
      "x-osl-ttl-seconds": ttl,
      ...(mode ? { "x-osl-expiry-mode": mode } : {}),
      "x-osl-blob-id": digest.slice(0, 32),
      "x-osl-fetch-digest": digest,
      "x-osl-ack-digest": await sha256Hex(`ack-${seed}`),
      "x-osl-manage-digest": await sha256Hex(`manage-${seed}`),
      "x-osl-delivery-tag": (await sha256Hex(`tag-${seed}`)).slice(0, 32),
      "x-osl-object-class": "single-ack",
    },
    body: new Uint8Array([1]),
  });
}

describe("seven-day default delivery floor", () => {
  it("refuses a default-mode TTL below seven days", async () => {
    expect((await handleUpload(await request("3600"), workerEnv())).status).toBe(400);
  });

  it("rejects a TTL outside the fixed allowlist", async () => {
    expect((await handleUpload(await request("3601"), workerEnv())).status).toBe(400);
  });

  it("accepts the seven-day default and an explicit absolute-mode opt-out", async () => {
    expect((await handleUpload(await request("604800"), workerEnv())).status).toBe(201);
    expect((await handleUpload(await request("3600", "absolute"), workerEnv())).status).toBe(201);
  });
});
