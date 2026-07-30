import { describe, expect, it } from "vitest";
import {
  controlInboxDispositionHealthError,
} from "./post-deploy-health-capability.mjs";

describe("post-deploy control-inbox schema capability", () => {
  it("accepts only the exact status-aware health response", () => {
    expect(
      controlInboxDispositionHealthError(200, {
        ok: true,
        capabilities: { control_inbox_sender_disposition: 1 },
      }),
    ).toBeNull();
  });

  it("refuses legacy, missing, wrong-version, and unhealthy responses", () => {
    expect(controlInboxDispositionHealthError(200, { ok: true })).toMatch(
      /not exactly 1/,
    );
    expect(
      controlInboxDispositionHealthError(200, {
        ok: true,
        capabilities: { control_inbox_sender_disposition: 0 },
      }),
    ).toMatch(/not exactly 1/);
    expect(
      controlInboxDispositionHealthError(200, {
        ok: true,
        capabilities: { control_inbox_sender_disposition: 2 },
      }),
    ).toMatch(/not exactly 1/);
    expect(
      controlInboxDispositionHealthError(503, {
        ok: false,
        capabilities: { control_inbox_sender_disposition: 0 },
      }),
    ).toMatch(/expected HTTP 200/);
  });
});
