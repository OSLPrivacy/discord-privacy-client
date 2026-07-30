import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

describe("POST /v1/username-coverage", () => {
  it("Define the frozen username-in / coverage-signal-categories-out respons", async () => {
    const res = await SELF.fetch("http://test/v1/username-coverage", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username: "alice.example_1" }),
    });

    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({
      version: 1,
      username: "alice.example_1",
      result_status: "not_scanned",
      coverage_signal_categories: [
        "public_profile_presence",
        "public_post_reference",
        "public_media_reference",
        "public_mention_reference",
        "contact_detail_exposure",
        "location_exposure",
        "credential_or_secret_exposure",
        "financial_or_identity_document_exposure",
        "sensitive_image_reference",
      ],
      signals: [],
    });
  });

  it("test/integration/username-coverage.test.ts", async () => {
    const res = await SELF.fetch("http://test/v1/username-coverage", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ username: "alice.example_1" }),
    });

    expect(res.status).toBe(200);
    expect(await res.json()).toEqual({
      version: 1,
      username: "alice.example_1",
      result_status: "not_scanned",
      coverage_signal_categories: [
        "public_profile_presence",
        "public_post_reference",
        "public_media_reference",
        "public_mention_reference",
        "contact_detail_exposure",
        "location_exposure",
        "credential_or_secret_exposure",
        "financial_or_identity_document_exposure",
        "sensitive_image_reference",
      ],
      signals: [],
    });
  });

  it("refuses missing, extra, and account-id-shaped username input", async () => {
    for (const body of [
      {},
      { username: "alice", platform: "discord" },
      { username: "123456789012345678" },
      { username: "alice example" },
    ]) {
      const res = await SELF.fetch("http://test/v1/username-coverage", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
      expect(res.status).toBe(400);
      expect(await res.json()).toHaveProperty("error");
    }
  });
});
