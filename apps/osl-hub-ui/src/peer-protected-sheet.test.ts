import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import type { HubPerson } from "./adapters";
import { LOCAL_TTL_OPTIONS } from "./local-protected-sheet";
import { blankPeerProtectedModel, boundedPeerProtectedDraft, peerProtectedDraftByteFeedback, peerProtectedSheetMarkup, verifiedPeerFriends } from "./peer-protected-sheet";

function person(overrides: Partial<HubPerson> = {}): HubPerson {
  return {
    personId: "person-1",
    oslUserId: "osl-user-1",
    alias: "Rose",
    safetyNumber: "1234 5678",
    safetyNumberVerified: true,
    whitelistCount: 0,
    whitelistedScopes: [],
    whitelistedScopesTruncated: false,
    pendingKeyChange: false,
    ...overrides,
  };
}

describe("manual peer protected sheet", () => {
  it("offers only verified stable friends and keeps local-only secondary", () => {
    const verified = person();
    const unverified = person({ personId: "person-2", alias: "Pat", safetyNumberVerified: false });
    const changed = person({ personId: "person-3", alias: "Sam", pendingKeyChange: true });
    expect(verifiedPeerFriends([verified, unverified, changed])).toEqual([verified]);

    const markup = peerProtectedSheetMarkup(blankPeerProtectedModel(true), [verified, unverified, changed]);
    expect(markup).toContain("Rose");
    expect(markup).not.toContain("Pat");
    expect(markup).not.toContain("Sam");
    expect(markup).toContain("Only this device");
    expect(markup).toContain("OSL does not read this page");
    expect(markup).not.toContain("peer-protect-form");
    expect(markup).not.toContain("Send");
  });

  it("requires one explicit app-and-friend approval before write or open controls exist", () => {
    const model = blankPeerProtectedModel(true);
    model.displayName = "Rose";
    model.personId = "person-1";
    model.context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-1",
      scopeApproved: false,
    };
    const markup = peerProtectedSheetMarkup(model, []);
    expect(markup).toContain("Approve app + friend");
    expect(markup).toContain("Limited to this app + friend");
    expect(markup).not.toContain(["app profile", "friend"].join(" + "));
    expect(markup).not.toContain("DM");
    expect(markup).not.toContain("peer-protect-form");
    expect(markup).not.toContain("peer-open-form");
    expect(markup).toContain("Nothing is sent automatically");
  });

  it("offers view-once explicitly and keeps it off by default", () => {
    const model = blankPeerProtectedModel(true);
    expect(model.viewOnce).toBe(false);
    model.context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-1",
      scopeApproved: true,
    };
    expect(peerProtectedSheetMarkup(model, [])).toContain('id="peer-protected-view-once"');
    model.viewOnce = true;
    expect(peerProtectedSheetMarkup(model, [])).toMatch(/id="peer-protected-view-once"[^>]*checked/u);
  });

  it("shows exact enforced timers and manual copy-paste truth after approval", () => {
    const model = blankPeerProtectedModel(true);
    model.displayName = "&lt;Rose&gt;";
    model.personId = "person-1";
    model.context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-1",
      scopeApproved: true,
    };
    model.coverText = "<protected>";
    const markup = peerProtectedSheetMarkup(model, []);
    expect(markup).toContain("&amp;lt;Rose&amp;gt;");
    expect(markup).toContain("&lt;protected&gt;");
    expect(markup).toContain("OSL copies protected text. It never presses Send.");
    expect(markup).toContain("Relay copy expires after");
    expect(markup).toContain("Copies already opened remain.");
    expect(markup).toContain("Manual copy & paste · person-to-person encryption · no page access");
    expect(markup).not.toContain("DM");
    expect(markup).not.toContain("exact recipient");
    expect(markup).not.toContain("No timer");
    for (const seconds of LOCAL_TTL_OPTIONS) expect(markup).toContain(`value="${seconds}"`);
  });

  it("bounds the write draft by UTF-8 bytes and reports newline and emoji boundaries truthfully", () => {
    const prefix = "a".repeat(996);
    expect(boundedPeerProtectedDraft(`${prefix}🔐`)).toBe(`${prefix}🔐`);
    expect(peerProtectedDraftByteFeedback(`${prefix}🔐`)).toBe("1000 / 1,000 bytes");
    expect(boundedPeerProtectedDraft(`${prefix}\n🔐`)).toBe(`${prefix}\n`);
    expect(peerProtectedDraftByteFeedback(boundedPeerProtectedDraft(`${prefix}\n🔐`))).toBe("997 / 1,000 bytes");
    expect(boundedPeerProtectedDraft(`${prefix}\n\n🔐`)).toBe(`${prefix}\n\n`);
    expect(boundedPeerProtectedDraft("🔐".repeat(251))).toBe("🔐".repeat(250));
  });

  it("renders model-backed multiline drafts and decrypted text without collapsing line breaks", () => {
    const model = blankPeerProtectedModel(true);
    model.displayName = "Rose";
    model.context = {
      contextToken: "ctx.peer-1",
      serviceId: "discord",
      accountId: "account-1",
      personId: "person-1",
      peerOslUserId: "osl-user-1",
      scopeApproved: true,
    };
    model.draft = "first line\nsecond 🔐";
    const writeMarkup = peerProtectedSheetMarkup(model, []);
    expect(writeMarkup).toContain("first line\nsecond 🔐</textarea>");
    expect(writeMarkup).toContain("22 / 1,000 bytes");
    expect(writeMarkup).not.toContain('maxlength="1000"');

    model.pane = "open";
    model.openDraft = "DPC0::first\n&lt;second&gt;";
    model.openedPlaintext = "opened first\nopened second";
    const openMarkup = peerProtectedSheetMarkup(model, []);
    expect(openMarkup).toContain("DPC0::first\n&amp;lt;second&amp;gt;</textarea>");
    expect(openMarkup).toContain("opened first\nopened second</p>");
    const styles = readFileSync(new URL("./local-protected-sheet.css", import.meta.url), "utf8");
    expect(styles).toMatch(/\.local-plaintext-result p[^}]*white-space:\s*pre-wrap/u);
  });

  it("labels authenticated direction without calling a prepared copy sent", () => {
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
    model.receipt = { direction: "sent", state: "prepared" };
    expect(peerProtectedSheetMarkup(model, [])).toContain("<span>You</span><strong>Prepared</strong>");
    expect(peerProtectedSheetMarkup(model, [])).not.toContain("<strong>Sent</strong>");

    model.receipt = { direction: "received", state: "received" };
    expect(peerProtectedSheetMarkup(model, [])).toContain("<span>Peer</span><strong>Received</strong>");

    model.receipt = { direction: "received", state: "opened-once" };
    expect(peerProtectedSheetMarkup(model, [])).toContain("Received · opened once");
  });

  it("stores both pane drafts on input before pane toggles and other rerenders", () => {
    const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    expect(source).toContain("peerProtectedSheet.draft = bounded;");
    expect(source).toContain("peerProtectedSheet.openDraft = peerOpenDraft.value;");
    expect(source).toMatch(/\[data-peer-pane\][\s\S]*?reconcilePeerDraft\(\);[\s\S]*?peerProtectedSheet\.openDraft = peerOpenDraft\.value;[\s\S]*?render\(\);/u);
    expect(source).toContain('if (opened.viewOnceConsumed) peerProtectedSheet.openDraft = "";');
  });

  it("keeps approval and policy persistence ahead of peer encryption in the controller", () => {
    const source = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
    const activateStart = source.indexOf("async function choosePeerProtectedFriend");
    const approveStart = source.indexOf("async function approvePeerProtectedDm");
    const prepareStart = source.indexOf("async function preparePeerProtectedDraft");
    const openStart = source.indexOf("async function openPeerProtectedText");
    const activation = source.slice(activateStart, approveStart);
    const approval = source.slice(approveStart, prepareStart);
    const prepare = source.slice(prepareStart, openStart);

    expect(activation).toContain("activateManualPeerContext");
    expect(activation).not.toContain("setActiveHubFriendPermission");
    expect(approval).toContain("setActiveHubFriendPermission(context.contextToken, context.personId, true, false)");
    expect(approval).toContain("Approved for this app + friend.");
    expect(prepare.indexOf("context?.scopeApproved")).toBeLessThan(prepare.indexOf("preparePeerProseText"));
    expect(prepare.indexOf("saveActiveContextSecurity")).toBeLessThan(prepare.indexOf("preparePeerProseText"));
    expect(prepare).toContain("navigator.clipboard.writeText(prepared.coverText)");
  });
});
