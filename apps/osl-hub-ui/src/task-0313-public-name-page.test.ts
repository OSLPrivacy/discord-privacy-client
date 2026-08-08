import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import { PublicNamePageController, type PublicNameCommands } from "./public-name-page";

function commands(): PublicNameCommands & {
  check: ReturnType<typeof vi.fn>;
  claim: ReturnType<typeof vi.fn>;
  cancel: ReturnType<typeof vi.fn>;
} {
  return {
    check: vi.fn(),
    claim: vi.fn(),
    cancel: vi.fn().mockResolvedValue(true),
  };
}

describe("TASK0313 public-name page", () => {
  it("is connected to Account settings while the raw legacy claim stays ACL-refused", () => {
    const main = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const rust = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
    const capability = JSON.parse(readFileSync(new URL("../../osl-hub/capabilities/hub.json", import.meta.url), "utf8")) as { permissions: string[] };
    expect(main).toContain("${publicNamePage.render()}");
    expect(main).toContain('"#public-name-check"');
    expect(main).toContain('"#public-name-claim"');
    expect(main).toContain('"#public-name-cancel"');
    expect(capability.permissions).toContain("allow-claim-checked-hub-username");
    expect(capability.permissions).not.toContain("allow-claim-hub-username");
    expect(rust).toContain('return Err("Check this exact public name before claiming it".to_owned());');
    expect(rust).toContain("proof.owner_user_id == owner && proof.username == username");
    console.log("TASK0313_RAW_CLAIM_ACL_GRANTS=0");
    console.log("TASK0313_CHECKED_CLAIM_EXACT_MATCH_GUARDS=2");
  });

  it("keeps Claim unavailable and refuses a direct method invoke before proof", async () => {
    const backend = commands();
    const page = new PublicNamePageController(backend);
    await page.enterName("alice_0313");

    expect(page.canClaim).toBe(false);
    expect(page.render()).toMatch(/id="public-name-claim"[^>]* disabled/u);
    await expect(page.claimName()).resolves.toBeNull();
    expect(backend.claim).toHaveBeenCalledTimes(0);
    console.log(`TASK0313_BEFORE_PROOF_CLAIM_ENABLED=${Number(page.canClaim)}`);
    console.log(`TASK0313_DIRECT_INVOKE_CALLS=${backend.claim.mock.calls.length}`);
  });

  it("enables Claim after the exact matching proof succeeds", async () => {
    const backend = commands();
    backend.check.mockResolvedValue({ username: "alice_0313", available: true, proofReady: true });
    backend.claim.mockResolvedValue({ username: "alice_0313", oslUserId: "osl-owner" });
    const page = new PublicNamePageController(backend);
    await page.enterName("alice_0313");

    await expect(page.checkName()).resolves.toBe(true);
    expect(page.canClaim).toBe(true);
    expect(page.render()).not.toMatch(/id="public-name-claim"[^>]* disabled/u);
    await expect(page.claimName()).resolves.toEqual({ username: "alice_0313", oslUserId: "osl-owner" });
    expect(backend.claim).toHaveBeenCalledWith("alice_0313");
    console.log(`TASK0313_MATCHING_PROOF_CLAIM_ENABLED=1`);
    console.log(`TASK0313_MATCHING_PROOF_NAME=${page.claimedName}`);
  });

  it("only the exact matching proof changes Claim availability", async () => {
    const backend = commands();
    backend.check.mockResolvedValue({ username: "neighbour_0313", available: true, proofReady: true });
    const page = new PublicNamePageController(backend);
    await page.enterName("alice_0313");

    await expect(page.checkName()).resolves.toBe(false);
    expect(page.canClaim).toBe(false);
    await expect(page.claimName()).resolves.toBeNull();
    expect(backend.claim).not.toHaveBeenCalled();

    backend.check.mockResolvedValue({ username: "alice_0313", available: false, proofReady: true });
    await expect(page.checkName()).resolves.toBe(false);
    expect(page.canClaim).toBe(false);
    console.log(`TASK0313_UNAVAILABLE_PROOF_CLAIM_ENABLED=${Number(page.canClaim)}`);

    backend.check.mockResolvedValue({ username: "alice_0313", available: true, proofReady: true });
    await expect(page.checkName()).resolves.toBe(true);
    expect(page.canClaim).toBe(true);
    await page.enterName("alice_0314");
    expect(page.canClaim).toBe(false);
    console.log(`TASK0313_WRONG_PROOF_CLAIM_ENABLED=0`);
    console.log(`TASK0313_CHANGED_NAME_CLAIM_ENABLED=${Number(page.canClaim)}`);
  });

  it("Cancel clears the entered name and matching proof", async () => {
    const backend = commands();
    backend.check.mockResolvedValue({ username: "alice_0313", available: true, proofReady: true });
    const page = new PublicNamePageController(backend);
    await page.enterName("alice_0313");
    await page.checkName();
    expect(page.canClaim).toBe(true);

    await page.cancel();
    expect(page.name).toBe("");
    expect(page.proofName).toBeNull();
    expect(page.canClaim).toBe(false);
    expect(backend.cancel).toHaveBeenCalled();
  });
});
