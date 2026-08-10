import { describe, expect, it } from "vitest";
import {
  UsernameNotAnalyzable,
  usernameClaimMessage,
  usernameSkeleton,
  validNormalizedUsername,
} from "../../src/lib/username.js";
import fixture from "../fixtures/username-skeletons.json";

describe("username canonicalization", () => {
  it("accepts only already-normalized bounded identifiers", () => {
    for (const value of ["a", "A", "liam_01", "_alice", "alice_"])
      expect(validNormalizedUsername(value)).toBe(true);
    for (const value of ["a".repeat(17), "alice.name", "alice-name", "alice name"])
      expect(validNormalizedUsername(value)).toBe(false);
  });

  // D-248. The skeleton is what makes `idx_username_directory_skeleton` bite.
  it("folds ASCII confusables that the shipping grammar accepts", () => {
    // Every one of these is claimable under USERNAME_RE, so each pair is a
    // reachable impersonation and not a hypothetical one.
    expect(usernameSkeleton("paypa1")).toBe(usernameSkeleton("paypal"));
    expect(usernameSkeleton("michae1")).toBe(usernameSkeleton("michael"));
    expect(usernameSkeleton("b0b")).toBe(usernameSkeleton("bob"));
    expect(usernameSkeleton("michael")).toBe("rnichael");
    expect(usernameSkeleton("michael")).not.toBe("michael");
    // and it must not fold names that merely look similar to a regex
    expect(usernameSkeleton("distinct_one")).not.toBe(usernameSkeleton("wholly_other"));
  });

  // The other half of the cross-runtime pin. `scripts/backfill-username-skeletons.test.ts`
  // asserts the node loader of the SAME wasm artifact against this same file,
  // so the Worker and the operator backfill cannot drift apart in silence.
  it("matches the cross-runtime vector set the backfill tool is held to", () => {
    expect(fixture.vectors.length).toBeGreaterThan(20);
    for (const [name, skeleton] of fixture.vectors as [string, string][]) {
      expect([name, usernameSkeleton(name)]).toEqual([name, skeleton]);
    }
  });

  // The canonicality tripwire is not decoration: starve it and it fires.
  it("refuses a handle outside the identifier profile", () => {
    expect(() => usernameSkeleton("ﬁnance")).toThrow(UsernameNotAnalyzable);
    expect(() => usernameSkeleton("")).toThrow(UsernameNotAnalyzable);
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
