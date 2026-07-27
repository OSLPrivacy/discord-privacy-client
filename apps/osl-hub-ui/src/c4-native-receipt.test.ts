import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  C4_NATIVE_RECEIPT_SCHEMA,
  captureC4NativeReceipt,
  serializeC4NativeReceiptEvidence,
  type NativeDiscordCarrierReceipt,
} from "./native-overlay-adapter";

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason?: unknown) => void;
} {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

const sentReceipt: NativeDiscordCarrierReceipt = {
  placed: true,
  enterSent: true,
  status: "sent",
  mode: "atomic",
  compatibilityDelayMs: 167,
};

describe("C4 native carrier receipt surface", () => {
  it("serializes only the existing native receipt fields", () => {
    const receiptWithRendererOnlyText = {
      ...sentReceipt,
      uiStatusText: "Sent privately through OSL.",
      inferredCarrierHash: "not-returned-by-rust",
    };

    expect(JSON.parse(serializeC4NativeReceiptEvidence(
      4,
      receiptWithRendererOnlyText,
    ))).toEqual({
      schema: C4_NATIVE_RECEIPT_SCHEMA,
      attempt: 4,
      state: "returned",
      receipt: sentReceipt,
    });
    expect(() => serializeC4NativeReceiptEvidence(0, sentReceipt)).toThrow(RangeError);
    expect(() => serializeC4NativeReceiptEvidence(
      Number.MAX_SAFE_INTEGER + 1,
      sentReceipt,
    )).toThrow(RangeError);
  });

  it("clears stale UI text to pending before invoke and binds the awaited native object", async () => {
    const native = deferred<NativeDiscordCarrierReceipt | null>();
    const surface = { value: "Sent privately through OSL." };
    const result = captureC4NativeReceipt(surface, 8, () => native.promise);

    // This assertion fails if the pending clear is deleted or moved after the
    // await: neither stale success nor the prior attempt may survive in flight.
    expect(JSON.parse(surface.value)).toEqual({
      schema: C4_NATIVE_RECEIPT_SCHEMA,
      attempt: 8,
      state: "pending",
    });

    const refusedReceipt: NativeDiscordCarrierReceipt = {
      placed: false,
      enterSent: false,
      status: "contextChanged",
      mode: "compatibility",
      compatibilityDelayMs: 200,
    };
    native.resolve(refusedReceipt);

    await expect(result).resolves.toBe(refusedReceipt);
    expect(JSON.parse(surface.value)).toEqual({
      schema: C4_NATIVE_RECEIPT_SCHEMA,
      attempt: 8,
      state: "returned",
      receipt: refusedReceipt,
    });
    expect(surface.value).not.toContain("Sent privately through OSL.");
  });

  it("publishes each attempt's actual result instead of reusing a prior receipt", async () => {
    const surface = { value: "" };
    await expect(captureC4NativeReceipt(
      surface,
      9,
      async () => sentReceipt,
    )).resolves.toBe(sentReceipt);

    const secondReceipt: NativeDiscordCarrierReceipt = {
      placed: false,
      enterSent: false,
      status: "composerNotEmpty",
      mode: "atomic",
      compatibilityDelayMs: 63,
    };
    await expect(captureC4NativeReceipt(
      surface,
      10,
      async () => secondReceipt,
    )).resolves.toBe(secondReceipt);
    expect(JSON.parse(surface.value)).toEqual({
      schema: C4_NATIVE_RECEIPT_SCHEMA,
      attempt: 10,
      state: "returned",
      receipt: secondReceipt,
    });
  });

  it("clears rather than fabricating evidence when native returns no receipt or rejects", async () => {
    const noReceiptSurface = { value: "stale" };
    await expect(captureC4NativeReceipt(
      noReceiptSurface,
      1,
      async () => null,
    )).resolves.toBeNull();
    expect(noReceiptSurface.value).toBe("");

    const rejectedSurface = { value: "stale" };
    await expect(captureC4NativeReceipt(
      rejectedSurface,
      2,
      async () => {
        throw new Error("native unavailable");
      },
    )).rejects.toThrow("native unavailable");
    expect(rejectedSurface.value).toBe("");
  });

  it("binds the shipping call site to the real native result and never the QA path", () => {
    const overlay = readFileSync(new URL("./overlay.ts", import.meta.url), "utf8");
    const html = readFileSync(new URL("../overlay.html", import.meta.url), "utf8");

    expect(html).toContain('id="native-carrier-receipt-evidence"');
    expect(overlay).toMatch(
      /captureC4NativeReceipt\(\s*c4NativeReceipt,\s*\+\+c4NativeReceiptAttempt,\s*\(\) => sendNativeDiscordOverlayCarrier\(/u,
    );
    expect(overlay).not.toMatch(
      /captureC4NativeReceipt\([\s\S]*?sendNativeDiscordQaAtomicText/u,
    );
    expect(overlay).not.toMatch(
      /serializeC4NativeReceiptEvidence\([^)]*(?:status\.textContent|carrierStatusLabel|markerSent)/u,
    );
  });
});
