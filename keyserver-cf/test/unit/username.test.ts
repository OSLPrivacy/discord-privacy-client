import { describe, expect, it } from "vitest";
import { usernameClaimMessage, validNormalizedUsername } from "../../src/lib/username.js";

describe("username canonicalization", () => {
  it("accepts only already-normalized bounded identifiers", () => {
    expect(validNormalizedUsername("liam_01")).toBe(true);
    for (const value of ["LiAm", "ab", "a".repeat(31), "_alice", "alice_", "alice.name", "alice-name", " alice"])
      expect(validNormalizedUsername(value)).toBe(false);
  });

  it("has a stable cross-client signing message", () => {
    expect(new TextDecoder().decode(usernameClaimMessage({
      username: "alice_01",
      user_id: "user-7",
      friend_code: "OSLFR1.invite",
      request_id: "A".repeat(43),
      timestamp_ms: 1_700_000_000_123,
    }))).toBe("OSL-USERNAME-CLAIM-v1\nalice_01\nuser-7\nOSLFR1.invite\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n1700000000123");
  });
});
