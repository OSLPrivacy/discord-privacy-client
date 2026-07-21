import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import {
  addOslFriendByUsername,
  claimOslUsername,
  isNormalizedOslUsername,
  parseHubAddFriendResult,
  parseHubUsernameClaim,
} from "./adapters";

describe("username friending adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("never silently normalizes usernames", () => {
    expect(isNormalizedOslUsername("alice_01")).toBe(true);
    for (const value of ["Alice", "ab", "_alice", "alice_", "alice.name", "alice-name"])
      expect(isNormalizedOslUsername(value)).toBe(false);
  });

  it("claims only a strictly parsed matching response", async () => {
    invoke.mockResolvedValue({ username: "alice_01", oslUserId: "user-1" });
    expect(await claimOslUsername("alice_01")).toEqual({ username: "alice_01", oslUserId: "user-1" });
    expect(invoke).toHaveBeenCalledWith("claim_hub_username", { username: "alice_01" });
    expect(parseHubUsernameClaim({ username: "Alice", oslUserId: "user-1" })).toBeNull();
    expect(await claimOslUsername("Alice")).toBeNull();
  });

  it("preserves safety-number verification after username resolution", async () => {
    const added = {
      disposition: "added", personId: "person-1", oslUserId: "user-2",
      safetyNumber: "123 456", codeSignatureValid: true, safetyNumberVerified: false,
    };
    invoke.mockResolvedValue(added);
    expect(await addOslFriendByUsername("bob_02", "Bob")).toEqual(added);
    expect(invoke).toHaveBeenCalledWith("add_hub_friend_by_username", {
      username: "bob_02", alias: "Bob",
    });
    expect(parseHubAddFriendResult({ ...added, safetyNumberVerified: true })).toBeNull();
    expect(parseHubAddFriendResult({ ...added, codeSignatureValid: false })).toBeNull();
  });
});
