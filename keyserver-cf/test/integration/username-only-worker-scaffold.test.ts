import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

async function postUsernameCoverage(body: unknown): Promise<Response> {
  return SELF.fetch("http://test/v1/username-coverage", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("username-only Worker scaffold", () => {
  it("test/integration/username-only-worker-scaffold.test.ts", async () => {
    const res = await postUsernameCoverage({ username: "alice.example_1" });
    expect(res.status).toBe(200);
    const body = await res.json();

    expect(body).toEqual({
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
    expect(body).not.toHaveProperty("provider");
    expect(body).not.toHaveProperty("account_id");
    expect(body).not.toHaveProperty("deletion");
    expect(body).not.toHaveProperty("risk_percentage");

    for (const rejected of [
      {},
      { username: "alice.example_1", provider: "discord" },
      { username: "123456789012345678" },
      { username: "alice@example.com:password" },
      { username: "alice example" },
    ]) {
      const denied = await postUsernameCoverage(rejected);
      expect(denied.status).toBe(400);
      expect(await denied.json()).toHaveProperty("error");
    }
  });
});
