import { env } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/lib/digest.js";
import { R2PayloadStore } from "../src/lib/payload-store.js";

describe("R2 payload store", () => {
  it("returns identical bytes, then reports not-found immediately after delete", async () => {
    const store = new R2PayloadStore(env.PAYLOADS);
    const fetchCap = "0123456789abcdef0123456789abcdef";
    const payload = new Uint8Array([0, 1, 2, 253, 254, 255]);

    await store.put(fetchCap, payload);
    expect(await env.PAYLOADS.head(await sha256Hex(fetchCap))).toMatchObject({ size: payload.byteLength });
    await expect(env.PAYLOADS.head(fetchCap)).resolves.toBeNull();
    await expect(store.get(fetchCap)).resolves.toEqual(payload);

    await store.delete(fetchCap);
    await expect(store.get(fetchCap)).resolves.toBeNull();
  });
});
