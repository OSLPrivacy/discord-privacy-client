import { describe, expect, it } from "vitest";
import { handleCloudAutoScrub } from "../../src/endpoints/cloud-autoscrub.js";

const scopeCommitment = btoa(String.fromCharCode(...new Uint8Array(32).fill(0x42)));

function request(body: unknown): Request {
  return new Request("http://test/v1/cloud-autoscrub", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

function nativeAuthority(overrides: Record<string, unknown> = {}) {
  return {
    native_run_id: "11111111-2222-4333-8444-555555555555",
    attended_authority_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
    operator_attended: true,
    scope_commitment_b64: scopeCommitment,
    ...overrides,
  };
}

describe("cloud AutoScrub authority admission", () => {
  it("test/integration/cloud-autoscrub.test.ts", async () => {
    const accepted = await handleCloudAutoScrub(
      request({ native_run_authority: nativeAuthority() }),
    );
    expect(accepted.status).toBe(200);
    await expect(accepted.json()).resolves.toEqual({
      status: "accepted",
      native_run_id: "11111111-2222-4333-8444-555555555555",
      scope_commitment_b64: scopeCommitment,
      cloud_authority_minted: false,
    });

    const missingNativeAuthority = await handleCloudAutoScrub(request({}));
    expect(missingNativeAuthority.status).toBe(400);
    await expect(missingNativeAuthority.json()).resolves.toMatchObject({
      error: "native_run_authority is required",
    });

    const unattended = await handleCloudAutoScrub(
      request({
        native_run_authority: nativeAuthority({ operator_attended: false }),
      }),
    );
    expect(unattended.status).toBe(400);
    await expect(unattended.json()).resolves.toMatchObject({
      error: "native_run_authority.operator_attended must be true",
    });

    const cloudMintAttempt = await handleCloudAutoScrub(
      request({
        native_run_authority: nativeAuthority(),
        cloud_authority: { native_run_id: "99999999-9999-4999-8999-999999999999" },
      }),
    );
    expect(cloudMintAttempt.status).toBe(400);
    await expect(cloudMintAttempt.json()).resolves.toMatchObject({
      error: "cloud AutoScrub cannot mint or accept cloud authority",
    });
  });
});
