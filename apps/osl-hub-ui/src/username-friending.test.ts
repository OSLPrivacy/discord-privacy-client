import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import {
  addOslFriendByUsername,
  claimOslUsername,
  getOslUsernameStatus,
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

  it("checks ownership without claiming and fails closed while offline", async () => {
    invoke.mockResolvedValueOnce({ username: "alice_01", ownedByActiveIdentity: true });
    await expect(getOslUsernameStatus("alice_01")).resolves.toEqual({ username: "alice_01", ownedByActiveIdentity: true });
    expect(invoke).toHaveBeenCalledWith("get_hub_username_status", { username: "alice_01" });
    invoke.mockRejectedValueOnce(new Error("offline"));
    await expect(getOslUsernameStatus("alice_01")).resolves.toBeNull();
  });

  it("fails closed when adding a username while offline", async () => {
    invoke.mockRejectedValueOnce(new Error("offline"));
    await expect(addOslFriendByUsername("bob_02", "Bob")).resolves.toBeNull();
  });
});
