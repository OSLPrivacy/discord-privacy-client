/// Tests for the test harness itself.
///
/// This file exists because of a specific failure. The R2 double used to accept
/// any `ReadableStream`, while workerd rejects a streamed `put`/`uploadPart`
/// body that has no known length. Every attachment upload returned 500 in
/// production for the entire life of the feature, and the suite was green the
/// whole time — the double could not fail, so it was decoration rather than
/// evidence.
///
/// These checks now run against the real pool-workers R2 binding. A guard
/// nothing exercises is in exactly the same position the old double was, so
/// these tests starve R2 of valid input and prove the runtime rule actually
/// fires. The describe title is historical and intentionally preserved.

import { describe, expect, it } from "vitest";
import { env } from "cloudflare:test";

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
    await expect(
      env.ATTACHMENTS.put("attachments/x", unknownLengthStream()),
    ).rejects.toThrow(/must have a known length/);
    expect(await env.ATTACHMENTS.head("attachments/x")).toBeNull();
  });

  it("rejects a multipart part with no known length", async () => {
    const upload = await env.ATTACHMENTS.createMultipartUpload("attachments/y");
    await expect(
      upload.uploadPart(1, unknownLengthStream()),
    ).rejects.toThrow(/must have a known length/);
  });

  it("still accepts a known-length body, so the guard is not simply refusing everything", async () => {
    const stored = await env.ATTACHMENTS.put("attachments/z", new Uint8Array([9, 9]));
    expect(stored?.size).toBe(2);
    expect(await env.ATTACHMENTS.head("attachments/z")).toMatchObject({ size: 2 });
  });

  it("honours onlyIf etagDoesNotMatch instead of silently overwriting", async () => {
    const first = await env.ATTACHMENTS.put("attachments/dup", new Uint8Array([1]), {
      onlyIf: { etagDoesNotMatch: "*" },
    });
    expect(first).not.toBeNull();
    // A real conditional put fails the precondition here; the endpoint reads a
    // null result as an id collision rather than a successful overwrite.
    const second = await env.ATTACHMENTS.put("attachments/dup", new Uint8Array([2]), {
      onlyIf: { etagDoesNotMatch: "*" },
    });
    expect(second).toBeNull();
    expect(await env.ATTACHMENTS.head("attachments/dup")).toMatchObject({ size: 1 });
  });
});
