import { describe, expect, it } from "vitest";
import { blankPeerProtectedModel, peerProtectedSheetMarkup } from "./peer-protected-sheet";
import {
  oslChatHandshakeConfirmed,
  oslChatsViewMarkup,
  type OslChatFriend,
  type OslChatMessage,
  type OslChatsViewModel,
} from "./osl-chats-view";
import {
  friendHandshakeDetail,
  friendHandshakeState,
  friendHandshakeSummary,
  friendInviteCardMarkup,
} from "./ui-behavior";

/**
 * Two defects, both of which strand a first-time user, both verified in the
 * shipping source before these tests were written:
 *
 * 1. There is NO inbound friend-request mechanism in the hub. `add_hub_friend`
 *    writes a record on this device only; `cmd_osl_send_friend_request` and
 *    `cmd_osl_accept_friend_request` live in crates/ipc and are not registered
 *    in apps/osl-hub/src/hub_command_surface.rs (the hub's only reference to
 *    them is inside `#[cfg(test)]`). Copy that says a request is pending makes
 *    the user wait for something that can never arrive.
 *
 * 2. `prepare_peer_prose_text` / `prepare_osl_chat_text` succeed whenever the
 *    SENDER has verified and approved. A peer who has done nothing has no
 *    manual binding (`ensure_friend_can_be_enabled`, security.rs) and cannot
 *    decrypt. The ciphertext is genuinely end-to-end encrypted, so the flag is
 *    not a cryptographic lie -- the defect is that an undeliverable-in-practice
 *    send is presented as a completed one.
 *
 * These assert rendered output and returned state only. None of them reads a
 * source file.
 */

/** Vocabulary that asserts something is inbound, en route, or owed by the peer. */
const INBOUND_PROMISE = /\bpending\b|\bawait|\bincoming\b|\bsent you\b|\bhas requested\b|\bwaiting (?:for|on) them\b|\bthey (?:will|have) (?:send|sent|request)/iu;

describe("no phantom inbound friend request (DEFECT 1)", () => {
  it("classifies an added-but-unverified person as an unfinished invite exchange, not a received request", () => {
    expect(friendHandshakeState(false, false)).toBe("invite-not-exchanged");
    expect(friendHandshakeState(true, false)).toBe("verified");
    expect(friendHandshakeState(true, true)).toBe("key-change");
    expect(friendHandshakeState(false, true)).toBe("key-change");
  });

  it("never promises an inbound request on any person row", () => {
    for (const [verified, pendingKeyChange] of [[false, false], [true, false], [false, true], [true, true]] as const) {
      expect(friendHandshakeSummary(verified, pendingKeyChange)).not.toMatch(INBOUND_PROMISE);
      expect(friendHandshakeDetail(verified, pendingKeyChange)).not.toMatch(INBOUND_PROMISE);
    }
  });

  it("puts the next action on the local user for a person who has only been added", () => {
    const summary = friendHandshakeSummary(false, false);
    // The ball is explicitly in the local user's court...
    expect(summary).toMatch(/\byou\b/iu);
    // ...and the action is named: send them YOUR invite, then verify.
    expect(summary).toMatch(/your invite/iu);
    expect(summary).toMatch(/verify/iu);
  });

  it("states plainly that OSL delivered nothing and that both people must act", () => {
    const detail = friendHandshakeDetail(false, false);
    expect(detail).toMatch(/no request/iu);
    expect(detail).toMatch(/send them your invite/iu);
    expect(detail).toMatch(/\bboth\b/iu);
    expect(detail).not.toMatch(INBOUND_PROMISE);
  });

  it("keeps the verified and key-change rows unchanged in meaning", () => {
    expect(friendHandshakeSummary(true, false)).toBe("Verified");
    expect(friendHandshakeSummary(false, true)).toMatch(/security change/iu);
    expect(friendHandshakeDetail(false, true)).toMatch(/protected sends stay off/iu);
  });

  it("renders an invite card whose note runs in both directions", () => {
    const markup = friendInviteCardMarkup("osl_abcd…wxyz", (value) => value, {
      sectionClass: "friend-invite people-invite",
      labelId: "people-friend-id-label",
      friendCode: "OSLFR1.ABCDEFGHIJKLMNOP",
    });
    // The card still does its original job.
    expect(markup).toContain('id="copy-friend-code"');
    expect(markup).toContain('aria-labelledby="people-friend-id-label"');
    expect(markup).toContain("osl_abcd…wxyz");
    // Outbound leg: they get yours.
    expect(markup).toMatch(/send your invite/iu);
    // Inbound leg, which the one-way copy omitted: you must add theirs too.
    expect(markup).toMatch(/add theirs/iu);
    expect(markup).toMatch(/both people/iu);
    // And it must not restate the false model it replaced.
    expect(markup).not.toMatch(INBOUND_PROMISE);
    expect(markup).not.toMatch(/so they can add you/iu);
  });

  it("escapes the friend id it renders", () => {
    const markup = friendInviteCardMarkup("<script>", (value) => value.replace(/</gu, "&lt;"), {
      sectionClass: "friend-invite",
      labelId: "friend-id-label",
      friendCode: "OSLFR1.ABCDEFGHIJKLMNOP",
    });
    expect(markup).toContain("&lt;script>");
  });
});

function chatFriend(overrides: Partial<OslChatFriend> = {}): OslChatFriend {
  return {
    personId: "friend-1",
    nickname: "Rose",
    verified: true,
    ready: true,
    preview: null,
    previewVisible: true,
    unreadCount: 0,
    ...overrides,
  };
}

const outgoing: OslChatMessage = {
  messageId: "m1",
  direction: "outgoing",
  body: "hello",
  state: "sent",
  timestampLabel: "Now",
};

const incoming: OslChatMessage = {
  messageId: "m2",
  direction: "incoming",
  body: "hi back",
  state: "received",
  timestampLabel: "Now",
};

function chatModel(overrides: Partial<OslChatsViewModel> = {}): OslChatsViewModel {
  return {
    friends: [chatFriend()],
    activePersonId: "friend-1",
    messages: [outgoing],
    draft: "",
    busy: false,
    ...overrides,
  };
}

describe("a one-way handshake is not a completed send (DEFECT 2)", () => {
  it("defaults to unconfirmed: an untouched friend record carries no reciprocity claim", () => {
    // The field is absent, not false, on every caller that predates this fix.
    expect(chatFriend().handshakeConfirmed).toBeUndefined();
    const markup = oslChatsViewMarkup(chatModel());
    expect(markup).toContain("osl-chat-handshake-warning");
  });

  it("treats only an inbound message as evidence the peer reciprocated", () => {
    expect(oslChatHandshakeConfirmed([])).toBe(false);
    // Sending is not evidence: encrypting and uploading succeed regardless of
    // whether the recipient ever bound a context.
    expect(oslChatHandshakeConfirmed([outgoing])).toBe(false);
    expect(oslChatHandshakeConfirmed([outgoing, outgoing])).toBe(false);
    expect(oslChatHandshakeConfirmed([incoming])).toBe(true);
    expect(oslChatHandshakeConfirmed([outgoing, incoming])).toBe(true);
  });

  it("warns that OSL cannot tell whether the recipient finished their half", () => {
    const markup = oslChatsViewMarkup(chatModel());
    expect(markup).toMatch(/nothing has ever arrived from Rose/iu);
    expect(markup).toMatch(/cannot tell whether they finished their half/iu);
    // It names the peer's three outstanding steps, which is the procedure.
    expect(markup).toMatch(/add your invite/iu);
    expect(markup).toMatch(/verify you/iu);
    expect(markup).toMatch(/turn this chat on/iu);
    expect(markup).toMatch(/both people must complete every step/iu);
  });

  it("does not present an unconfirmed outgoing message as a finished delivery", () => {
    const markup = oslChatsViewMarkup(chatModel());
    expect(markup).toContain("osl-chat-message-unreadable");
    expect(markup).toContain("Not readable yet");
  });

  it("drops both the warning and the per-message caveat once the peer has answered", () => {
    const markup = oslChatsViewMarkup(chatModel({
      friends: [chatFriend({ handshakeConfirmed: true })],
      messages: [outgoing, incoming],
    }));
    expect(markup).not.toContain("osl-chat-handshake-warning");
    expect(markup).not.toContain("Not readable yet");
    // The ordinary delivery tag is untouched.
    expect(markup).toContain('class="osl-chat-message-state is-sent">Sent</span>');
  });

  it("does not caveat an incoming message, which by definition arrived", () => {
    const markup = oslChatsViewMarkup(chatModel({ messages: [incoming] }));
    expect(markup).toContain('class="osl-chat-message-state is-received">Received</span>');
    expect(markup).not.toContain("osl-chat-message-unreadable");
  });

  it("stays quiet for an unverified friend, whose blocker is verification, not reciprocity", () => {
    const markup = oslChatsViewMarkup(chatModel({ friends: [chatFriend({ verified: false, ready: false })] }));
    expect(markup).not.toContain("osl-chat-handshake-warning");
    expect(markup).toContain("Verify this friend to chat.");
  });

  it("carries the same caveat on the prepared carrier text", () => {
    const model = blankPeerProtectedModel(true);
    // Unconfirmed is the default state of a fresh sheet.
    expect(model.handshakeConfirmed).toBe(false);
    model.displayName = "Peer";
    model.context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-1",
      scopeApproved: true,
    };
    model.coverText = "OSLPTR1.abc";

    const unconfirmed = peerProtectedSheetMarkup(model, []);
    expect(unconfirmed).toContain("peer-handshake-warning");
    expect(unconfirmed).toMatch(/nothing has ever been opened from Peer here/iu);
    expect(unconfirmed).toMatch(/stays unreadable for them however you send it/iu);

    model.handshakeConfirmed = true;
    const confirmed = peerProtectedSheetMarkup(model, []);
    expect(confirmed).not.toContain("peer-handshake-warning");
    expect(confirmed).toContain("OSLPTR1.abc");
  });

  it("says nothing about readability before a carrier has been prepared", () => {
    const model = blankPeerProtectedModel(true);
    model.displayName = "Peer";
    model.context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-1",
      scopeApproved: true,
    };
    expect(peerProtectedSheetMarkup(model, [])).not.toContain("peer-handshake-warning");
  });
});
