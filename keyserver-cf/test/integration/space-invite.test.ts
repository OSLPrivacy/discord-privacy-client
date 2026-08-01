import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";

const encoder = new TextEncoder();

async function capability(label: string): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", encoder.encode(label)),
  );
  return btoa(String.fromCharCode(...digest))
    .replaceAll("+", "-")
    .replaceAll("/", "_")
    .replaceAll("=", "");
}

async function request(
  path: string,
  method: string,
  body: Record<string, unknown>,
): Promise<Response> {
  return SELF.fetch(`http://test${path}`, {
    method,
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("Space invite bearer capabilities", () => {
  it("consumes exactly once and a revocation survives the inviter going offline", async () => {
    const suffix = `${Date.now()}-${Math.random()}`;
    const firstInvite = await capability(`first-invite-${suffix}`);
    const firstRevoke = await capability(`first-revoke-${suffix}`);
    const secondInvite = await capability(`second-invite-${suffix}`);
    const secondRevoke = await capability(`second-revoke-${suffix}`);
    const expiresAt = Math.floor(Date.now() / 1000) + 60;

    expect((await request("/v1/space-invite", "POST", {
      capability: firstInvite,
      revocation_capability: firstRevoke,
      expires_at: expiresAt,
    })).status).toBe(201);

    expect((await request("/v1/space-invite/consume", "POST", {
      capability: firstInvite,
    })).status).toBe(204);
    expect((await request("/v1/space-invite/consume", "POST", {
      capability: firstInvite,
    })).status).toBe(404);

    expect((await request("/v1/space-invite", "POST", {
      capability: secondInvite,
      revocation_capability: secondRevoke,
      expires_at: expiresAt,
    })).status).toBe(201);
    expect((await request("/v1/space-invite", "DELETE", {
      revocation_capability: secondRevoke,
    })).status).toBe(204);
    expect((await request("/v1/space-invite/consume", "POST", {
      capability: secondInvite,
    })).status).toBe(404);
  });
});
