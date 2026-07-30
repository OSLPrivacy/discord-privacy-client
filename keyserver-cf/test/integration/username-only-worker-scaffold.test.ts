import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

async function usernameCoverage(body: unknown): Promise<Response> {
  return SELF.fetch("http://test/v1/username-coverage", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("username-only Worker scaffold", () => {
  it("test/integration/username-only-worker-scaffold.test.ts", async () => {
    const acceptedUsername = "alice.example_1";
    const accepted = await usernameCoverage({ username: acceptedUsername });
    expect(accepted.status).toBe(200);
    const payload = await accepted.json() as Record<string, unknown>;
    expect(payload).toEqual({
      version: 1,
      username: acceptedUsername,
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
    for (const forbidden of [
      "deletion",
      "private_mailbox_access",
      "browser_profile_access",
      "calibrated_risk_percentage",
    ]) {
      expect(JSON.stringify(payload)).not.toMatch(new RegExp(forbidden, "iu"));
    }

    const refusals: Array<[string, unknown]> = [
      ["missing_username", {}],
      ["extra_provider", { username: acceptedUsername, provider: "discord" }],
      ["credential_like_input", { username: "alice:login" }],
      [
        "unsupported_provider_binding",
        { username: acceptedUsername, account_id: "900000000000000001" },
      ],
      ["discord_snowflake", { username: "900000000000000001" }],
    ];
    expect(refusals.map(([name]) => name).sort()).toEqual(
      [
        "credential_like_input",
        "discord_snowflake",
        "extra_provider",
        "missing_username",
        "unsupported_provider_binding",
      ],
    );
    for (const [, body] of refusals) {
      const refused = await usernameCoverage(body);
      expect(refused.status).toBe(400);
      await expect(refused.json()).resolves.toHaveProperty("error");
    }
  });
});
