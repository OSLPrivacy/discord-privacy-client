import { describe, expect, it } from "vitest";
import { sha256Hex } from "../src/lib/digest.js";
import { applyBurn, type BurnStore } from "../src/lib/burn-policy.js";

const MANAGE_CAP = "a".repeat(32);

function storeWith(...ids: string[]): { store: BurnStore; has: (id: string) => boolean } {
  const rows = new Map<string, string>();
  const ready = Promise.resolve(sha256Hex(MANAGE_CAP)).then((value) => {
    for (const id of ids) rows.set(id, value);
  });

  return {
    store: {
      async manageCapabilityDigestFor(id) {
        await ready;
        return rows.get(id) ?? null;
      },
      async destroy(id) {
        await ready;
        rows.delete(id);
      },
    },
    has: (id) => rows.has(id),
  };
}

describe("offline queued blob burns", () => {
  it("is idempotent after TTL sweep, ACK, and repeated delivery", async () => {
    const swept = storeWith();
    const acked = storeWith();
    const repeated = storeWith("33333333333333333333333333333333");
    const wrongCapability = storeWith("55555555555555555555555555555555");

    await expect(applyBurn(swept.store, "11111111111111111111111111111111", MANAGE_CAP))
      .resolves.toMatchObject({ status: 204 });
    await expect(applyBurn(acked.store, "22222222222222222222222222222222", MANAGE_CAP))
      .resolves.toMatchObject({ status: 204 });
    await expect(applyBurn(wrongCapability.store, "55555555555555555555555555555555", "b".repeat(32)))
      .resolves.toMatchObject({ status: 204 });
    expect(wrongCapability.has("55555555555555555555555555555555")).toBe(true);

    for (let attempt = 0; attempt < 3; attempt++) {
      await expect(applyBurn(repeated.store, "33333333333333333333333333333333", MANAGE_CAP))
        .resolves.toMatchObject({ status: 204 });
    }
    expect(repeated.has("33333333333333333333333333333333")).toBe(false);
  });

  it("accepts a manage capability kept in an offline queue for six days", async () => {
    const pending = storeWith("44444444444444444444444444444444");
    const queued = {
      blobId: "44444444444444444444444444444444",
      manageCapability: MANAGE_CAP,
      queuedAt: Date.now() - (6 * 24 * 60 * 60 * 1000),
    };
    expect(queued.queuedAt).toBeLessThan(Date.now());

    await expect(applyBurn(pending.store, queued.blobId, queued.manageCapability))
      .resolves.toMatchObject({ status: 204 });
    expect(pending.has("44444444444444444444444444444444")).toBe(false);
  });
});
