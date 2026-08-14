import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: () => true }));

import { createOslFriendRequestByOslName, isValidPublicOslName } from "./adapters";
import {
  addFriendByNameBoxMarkup,
  addPendingFriendRequest,
  friendNameRequestRefusal,
  pendingFriendRequestListMarkup,
  submitFriendNameRequest,
  type PendingFriendRequestEntry,
} from "./ui-behavior";

const escapeHtml = (value: string) => value.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);

/**
 * A fixture standing in for the backend: exactly one public OSL name is
 * known ("maple_0213"). Anything else -- including names that merely look
 * plausible -- is unknown and must be refused, never silently accepted.
 */
function fixtureCreateRequest(knownName: string) {
  return async (name: string): Promise<PendingFriendRequestEntry | null> => {
    if (name !== knownName) return null;
    return { requestId: `req-${name}`, recipientName: name };
  };
}

describe("add-friend name request box", () => {
  beforeEach(() => invoke.mockReset());

  it("renders the name box, the Send Request action, and an initially empty pending list", () => {
    const markup = addFriendByNameBoxMarkup();
    expect(markup).toContain('id="friend-osl-name-input"');
    expect(markup).toContain('id="send-friend-request-by-name"');
    expect(markup).toContain("Send Request");
    expect(markup).toContain('id="pending-friend-requests-list"');
    expect(pendingFriendRequestListMarkup([], escapeHtml)).toBe("");
  });

  it("submitting one known name displays exactly one pending entry naming that name, and the count goes from 0 to 1", async () => {
    let pending: PendingFriendRequestEntry[] = [];
    expect(pending.length).toBe(0);

    const outcome = await submitFriendNameRequest("maple_0213", {
      createRequest: fixtureCreateRequest("maple_0213"),
    });

    expect(outcome.ok).toBe(true);
    if (outcome.ok) pending = addPendingFriendRequest(pending, outcome.entry);

    expect(pending.length).toBe(1);
    expect(pending).toEqual([{ requestId: "req-maple_0213", recipientName: "maple_0213" }]);

    const markup = pendingFriendRequestListMarkup(pending, escapeHtml);
    const matches = markup.match(/class="pending-friend-request"/g) ?? [];
    expect(matches.length).toBe(1);
    expect(markup).toContain("Pending: maple_0213");
  });

  it("submitting an unknown name adds zero pending entries and is refused by name", async () => {
    let pending: PendingFriendRequestEntry[] = [];

    const outcome = await submitFriendNameRequest("nobody_here", {
      createRequest: fixtureCreateRequest("maple_0213"),
    });

    expect(outcome.ok).toBe(false);
    if (!outcome.ok) {
      expect(outcome.refusal).toBe(friendNameRequestRefusal("nobody_here"));
      expect(outcome.refusal).toContain("nobody_here");
    }

    // Nothing is added to the pending list on refusal.
    expect(pending.length).toBe(0);
    expect(pendingFriendRequestListMarkup(pending, escapeHtml)).toBe("");
  });

  it("refuses a blank name locally, without ever calling the backend", async () => {
    const createRequest = vi.fn(fixtureCreateRequest("maple_0213"));
    const outcome = await submitFriendNameRequest("   ", { createRequest });
    expect(outcome.ok).toBe(false);
    if (!outcome.ok) expect(outcome.refusal).toMatch(/Enter an OSL name/);
    expect(createRequest).not.toHaveBeenCalled();
  });

  it("validates public OSL names the same way the backend does", () => {
    expect(isValidPublicOslName("maple_0213")).toBe(true);
    expect(isValidPublicOslName("")).toBe(false);
    expect(isValidPublicOslName("a".repeat(65))).toBe(false);
    expect(isValidPublicOslName("has space")).toBe(false);
    expect(isValidPublicOslName("has/slash")).toBe(false);
    expect(isValidPublicOslName("has\\backslash")).toBe(false);
  });

  it("adapter: a known OSL name produces exactly one pending record from the backend", async () => {
    invoke.mockImplementationOnce(async (_command: string, args: Record<string, unknown>) =>
      ({
        requestId: args.requestId,
        recipientName: args.recipientName,
        peerOslUserId: "user-maple",
        pending: { peerDiscordId: "user-maple", scopeStorageKey: "dm:user-maple", createdAtUnixSeconds: 1_700_000_000 },
      }));

    const result = await createOslFriendRequestByOslName("maple_0213");
    expect(result).not.toBeNull();
    expect(result?.recipientName).toBe("maple_0213");
    expect(invoke).toHaveBeenCalledWith(
      "create_friend_request_by_osl_name",
      expect.objectContaining({ recipientName: "maple_0213" }),
    );
  });

  it("adapter: an unknown OSL name is refused, not silently accepted", async () => {
    invoke.mockRejectedValueOnce(new Error("OSL: unknown OSL name"));
    const result = await createOslFriendRequestByOslName("nobody_here");
    expect(result).toBeNull();
  });

  it("adapter: fails closed while offline", async () => {
    invoke.mockRejectedValueOnce(new Error("offline"));
    const result = await createOslFriendRequestByOslName("maple_0213");
    expect(result).toBeNull();
  });
});
