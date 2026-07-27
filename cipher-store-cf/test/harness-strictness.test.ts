/// Tests for the test harness itself.
///
/// This file exists because of a specific failure. The R2 double used to accept
/// any `ReadableStream`, while workerd rejects a streamed `put`/`uploadPart`
/// body that has no known length. Every attachment upload returned 500 in
/// production for the entire life of the feature, and the suite was green the
/// whole time — the double could not fail, so it was decoration rather than
/// evidence.
///
/// The double is now strict. A guard nothing exercises is in exactly the same
/// position the old double was, so these tests starve it of valid input and
/// prove it actually fires.

import { describe, expect, it } from "vitest";
import { memoryR2 } from "./helpers/d1.js";

function unknownLengthStream(): ReadableStream<Uint8Array> {
  // A pipeThrough result carries no length — this is the exact shape the
  // shipping code used to hand to R2.
  return new ReadableStream<Uint8Array>({
    start(controller) {
      controller.enqueue(new Uint8Array([1, 2, 3, 4]));
      controller.close();
    },
  }).pipeThrough(new TransformStream<Uint8Array, Uint8Array>());
}

describe("the R2 double refuses what production refuses", () => {
  it("rejects a put body with no known length", async () => {
    const r2 = memoryR2();
    await expect(
      r2.bucket.put("attachments/x", unknownLengthStream() as never),
    ).rejects.toThrow(/must have a known length/);
    expect(r2.objects.size).toBe(0);
  });

  it("rejects a multipart part with no known length", async () => {
    const r2 = memoryR2();
    const upload = await r2.bucket.createMultipartUpload("attachments/y");
    await expect(
      upload.uploadPart(1, unknownLengthStream() as never),
    ).rejects.toThrow(/must have a known length/);
  });

  it("still accepts a known-length body, so the guard is not simply refusing everything", async () => {
    const r2 = memoryR2();
    const stored = await r2.bucket.put("attachments/z", new Uint8Array([9, 9]));
    expect(stored?.size).toBe(2);
    expect(r2.objects.get("attachments/z")).toEqual(new Uint8Array([9, 9]));
  });

  it("honours onlyIf etagDoesNotMatch instead of silently overwriting", async () => {
    const r2 = memoryR2();
    const first = await r2.bucket.put("attachments/dup", new Uint8Array([1]), {
      onlyIf: { etagDoesNotMatch: "*" },
    });
    expect(first).not.toBeNull();
    // A real conditional put fails the precondition here; the endpoint reads a
    // null result as an id collision rather than a successful overwrite.
    const second = await r2.bucket.put("attachments/dup", new Uint8Array([2]), {
      onlyIf: { etagDoesNotMatch: "*" },
    });
    expect(second).toBeNull();
    expect(r2.objects.get("attachments/dup")).toEqual(new Uint8Array([1]));
  });
});
