/**
 * Two product defects, held down by behaviour rather than by source text.
 *
 * 1. The invite could not be exported anywhere except Windows. The backend
 *    refused off Windows and the raw `OSLFR1.` string was rendered nowhere, so
 *    on Linux there was no path at all to hand your invite to a friend. These
 *    tests read the *rendered* card back and assert the whole invite is the
 *    text of an element in it, and that the operator can reach and select it.
 *
 * 2. The real reason was swallowed twice: a failed copy said "Could not copy
 *    the invite" and a bad invite said "Nothing changed", both discarding what
 *    the backend actually reported. These tests assert the returned state and
 *    the composed on-screen strings, never the source of the functions.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { addOslFriend, copyHubFriendInvite, INVITE_NICKNAME_REFUSED, INVITE_NOT_RECOGNISED } from "./adapters";
import { addFriendFailureStatus, friendInviteCardMarkup, inviteCopyFailureToast } from "./ui-behavior";
import { clearBackendFailures } from "./backend-failure";

/** A realistic invite: the long unbroken base64url body is the whole problem. */
const INVITE = `OSLFR1.${"eyJwYXlsb2FkIjp7InZlcnNpb24iOjEsIm9zbF91c2VyX2lkIjoib3NsX3Rlc3RfaWRlbnRpdHkifX0"}`;

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;");
}

/**
 * The card is rendered as markup, and this workspace has no DOM
 * implementation, so these helpers read the rendered *output* back: the text
 * of the element that carries the invite, entity-decoded, and the words a
 * person actually sees with the tags taken out. Nothing here inspects the
 * source of the function under test.
 */
function decodeEntities(value: string): string {
  return value
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&quot;/gu, "\"")
    .replace(/&#39;/gu, "'")
    .replace(/&amp;/gu, "&");
}

function renderCard(friendCode: string): string {
  return friendInviteCardMarkup("osl_abcd…wxyz", escapeHtml, {
    sectionClass: "friend-invite people-invite",
    labelId: "people-friend-id-label",
    friendCode,
  });
}

/** The rendered text of the element carrying the invite. */
function renderedInviteText(markup: string): string {
  const match = /<code[^>]*\bdata-friend-invite\b[^>]*>([\s\S]*?)<\/code>/u.exec(markup);
  if (!match) throw new Error("the card rendered no element carrying the invite");
  return decodeEntities(match[1] as string);
}

function inviteElementAttribute(markup: string, name: string): string | null {
  const tag = /<code([^>]*\bdata-friend-invite\b[^>]*)>/u.exec(markup);
  if (!tag) throw new Error("the card rendered no element carrying the invite");
  const attribute = new RegExp(`\\b${name}="([^"]*)"`, "u").exec(tag[1] as string);
  return attribute ? decodeEntities(attribute[1] as string) : null;
}

/** Everything a person reads on the card, markup removed. */
function visibleText(markup: string): string {
  return decodeEntities(markup.replace(/<[^>]*>/gu, " ")).replace(/\s+/gu, " ").trim();
}

beforeEach(() => {
  mocks.invoke.mockReset();
  mocks.isTauriRuntime.mockReturnValue(true);
  clearBackendFailures();
});

describe("defect 1 — the invite must be obtainable without a clipboard", () => {
  it("renders the whole invite as text, not only the shortened friend ID", () => {
    // The rendered text, read back out of the card, must be the exact value
    // `export_friend_code` produced -- no ellipsis, no compacting, no new
    // format. That is what makes it usable by hand.
    expect(renderedInviteText(renderCard(INVITE))).toBe(INVITE);
  });

  it("keeps the shortened friend ID as a label beside the usable invite", () => {
    const card = renderCard(INVITE);

    // The compact form is still shown, but it is no longer the only thing
    // shown: it is not a value `add_hub_friend` would ever accept.
    expect(visibleText(card)).toContain("osl_abcd…wxyz");
    expect(renderedInviteText(card)).toBe(INVITE);
  });

  it("offers the invite to the keyboard as well as the mouse", () => {
    const card = renderCard(INVITE);

    // Reachable by tab, and named, so it can be selected and read out without
    // a pointer at all.
    expect(inviteElementAttribute(card, "tabindex")).toBe("0");
    expect(inviteElementAttribute(card, "aria-labelledby")).toBe("people-friend-id-label-full-invite");
  });

  it("escapes the invite it renders rather than trusting its shape", () => {
    const hostile = 'OSLFR1.<img src=x onerror="boom">';
    const card = renderCard(hostile);

    expect(card).not.toContain("<img");
    // Escaped on the way in, and still the same characters on the way out.
    expect(renderedInviteText(card)).toBe(hostile);
  });

  it("still tells the operator the invite is on screen when copying is refused", () => {
    // The note has to point at the fallback, otherwise a failed copy still
    // reads as a dead end.
    expect(visibleText(renderCard(INVITE))).toMatch(/select the invite above/iu);
  });
});

describe("defect 2 — the backend's reason must reach the operator", () => {
  it("returns the native refusal instead of a bare false when copying fails", async () => {
    mocks.invoke.mockRejectedValueOnce(
      "This desktop has no clipboard tool OSL can use (wl-copy, xclip, xsel or pbcopy).",
    );

    const result = await copyHubFriendInvite(INVITE);

    expect(result.copied).toBe(false);
    expect(result.reason).toContain("no clipboard tool");
  });

  it("puts that reason on screen beside the generic sentence", async () => {
    mocks.invoke.mockRejectedValueOnce("OSL identity is not loaded");

    const toast = inviteCopyFailureToast((await copyHubFriendInvite(INVITE)).reason);

    expect(toast).toContain("Could not copy the invite");
    expect(toast).toContain("OSL identity is not loaded");
  });

  it("falls back to the generic sentence alone when there is genuinely no reason", () => {
    expect(inviteCopyFailureToast("")).toBe("Could not copy the invite");
    expect(inviteCopyFailureToast("   ")).toBe("Could not copy the invite");
  });

  it("distinguishes the three add-friend failures the operator could not tell apart", async () => {
    // A truncated or mistyped paste never reaches the backend at all.
    const notAnInvite = await addOslFriend("OSLFR1.short", "Rose");
    expect(notAnInvite).toEqual({ added: false, reason: INVITE_NOT_RECOGNISED });
    expect(mocks.invoke).not.toHaveBeenCalled();

    // A valid invite with an unusable local nickname is a different mistake.
    const badNickname = await addOslFriend(INVITE, "x".repeat(200));
    expect(badNickname).toEqual({ added: false, reason: INVITE_NICKNAME_REFUSED });
    expect(mocks.invoke).not.toHaveBeenCalled();

    // And an identity-key change is the backend's own sentence, verbatim.
    mocks.invoke.mockRejectedValueOnce(
      "OSL refuses this invite: it claims a friend you already have but carries a different identity key",
    );
    const keyChange = await addOslFriend(INVITE, "Rose");
    expect(keyChange.added).toBe(false);
    expect(keyChange.reason).toContain("carries a different identity key");
  });

  it("keeps 'nothing changed' true while saying why", () => {
    const status = addFriendFailureStatus("OSL friend code signature is invalid");

    expect(status).toContain("Nothing changed.");
    expect(status).toContain("OSL friend code signature is invalid");
    expect(addFriendFailureStatus("")).toBe("The invite could not be added. Nothing changed.");
  });

  it("reports a successful add as added with no reason to show", async () => {
    mocks.invoke.mockResolvedValueOnce(undefined);

    await expect(addOslFriend(INVITE, "Rose")).resolves.toEqual({ added: true, reason: "" });
  });
});

describe("what the surfaced reason deliberately withholds", () => {
  it("redacts opaque key material a backend message echoed back", async () => {
    const leakedKey = "QUJDREVGR0hJSktMTU5PUFFSU1RVVldYWVphYmNkZWZnaGlqa2xtbm9w";
    mocks.invoke.mockRejectedValueOnce(`OSL friend code could not be verified: ${leakedKey}`);

    const result = await addOslFriend(INVITE, "Rose");

    expect(result.reason).toContain("could not be verified");
    expect(result.reason).not.toContain(leakedKey);
  });

  it("redacts a labelled credential rather than repeating it in a status line", async () => {
    mocks.invoke.mockRejectedValueOnce("OSL keyserver refused: token=hunter2");

    const reason = (await addOslFriend(INVITE, "Rose")).reason;

    expect(reason).toContain("keyserver refused");
    expect(reason).not.toContain("hunter2");
  });

  it("bounds a hostile backend message so it cannot flood the status line", async () => {
    mocks.invoke.mockRejectedValueOnce(`refused ${"why ".repeat(400)}`);

    const reason = (await addOslFriend(INVITE, "Rose")).reason;

    expect(Array.from(reason).length).toBeLessThanOrEqual(241);
  });

  it("collapses a multi-line refusal into one status line", async () => {
    mocks.invoke.mockRejectedValueOnce("OSL friend code is malformed\n\tat security.rs:3426");

    const reason = (await addOslFriend(INVITE, "Rose")).reason;

    expect(reason).toContain("OSL friend code is malformed");
    expect(reason).not.toContain("\n");
  });
});
