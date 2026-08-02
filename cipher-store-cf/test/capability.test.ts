import { describe, expect, it } from "vitest";
import vectors from "./fixtures/pointer-vectors.json";
import { derivePointerCapabilities } from "../src/lib/capability.js";

interface Vector {
  p: string;
  k_msg: string;
  k_send: string;
  k_conv_n: string;
  blob_id: string;
  fetch_cap: string;
  ack_cap: string;
  manage_cap: string;
  tag_n: string;
}

function fromHex(value: string): Uint8Array {
  return Uint8Array.from(value.match(/../g) ?? [], (pair) => Number.parseInt(pair, 16));
}

describe("160-bit pointer derivation", () => {
  const sharedVectors = vectors as Vector[];
  expect(sharedVectors).toHaveLength(20);

  it.each(sharedVectors)("matches published T1/T6 vector %#", async (vector) => {
    await expect(derivePointerCapabilities({
        pointer: fromHex(vector.p),
        messageKey: fromHex(vector.k_msg),
        sendKey: fromHex(vector.k_send),
        conversationKey: fromHex(vector.k_conv_n),
    })).resolves.toEqual({
      blobId: vector.blob_id,
      fetchCap: vector.fetch_cap,
      ackCap: vector.ack_cap,
      manageCap: vector.manage_cap,
      deliveryTag: vector.tag_n,
    });
  });

  it("rejects a pointer that is not 160 bits", async () => {
    await expect(derivePointerCapabilities({
      pointer: new Uint8Array(19),
      messageKey: new Uint8Array(32),
      sendKey: new Uint8Array(32),
      conversationKey: new Uint8Array(32),
    })).rejects.toThrow("160 bits");
  });
});
