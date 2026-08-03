import { describe, expect, it } from "vitest";
import { OSL_MAIL_DELETION_UNVERIFIED_NOTE, oslMailViewMarkup, type OslMailViewModel } from "./osl-mail-view";

/**
 * Master §7.5: "Requested deletion is never displayed as verified deletion."
 *
 * Any sentence that puts a deletion word and `confirmed`/`verified` together is
 * the prohibited claim.  `[^.]` keeps the match inside one sentence so that a
 * later, separate sentence stating the limitation cannot mask an earlier lie.
 * `\bverified\b` deliberately does not match `unverified`.
 */
const VERIFIED_DELETION_CLAIM =
  /\b(deletion|deleted|delete|burn|burned|erasure|erased|removal|removed)\b[^.]{0,80}\b(confirmed|verified)\b|\b(confirmed|verified)\b[^.]{0,80}\b(deletion|deleted|delete|burn|burned|erasure|erased|removal|removed)\b/iu;

const base: OslMailViewModel = { loading: false, available: false, signedUsername: "liam", status: null, threads: [], activeThread: null, pane: "inbox", notifications: true, deleteReceipt: null, sendReceipt: null, burnReceipt: null, error: null };
const status = { available: true as const, provisioned: true as const, address: "liam@oslprivacy.com" as const, unreadCount: 0, retentionSeconds: 3600 };

describe("OSL Mail view", () => {
  it("truthfully renders backend unavailability without fixtures", () => {
    const html = oslMailViewMarkup(base);
    expect(html).toContain("OSL Mail unavailable");
    expect(html).toContain("No messages or account state are being invented");
  });

  it("derives provisioning from the signed OSL username with no phone field", () => {
    const html = oslMailViewMarkup({ ...base, available: true });
    expect(html).toContain("liam@oslprivacy.com");
    expect(html).not.toContain('type="tel"');
    expect(html).not.toContain('id="osl-mail-phone"');
  });

  it("keeps external outbound unavailable and explains external inbound as ordinary email", () => {
    const compose = oslMailViewMarkup({ ...base, available: true, status, pane: "compose" });
    const settings = oslMailViewMarkup({ ...base, available: true, status, pane: "settings" });
    expect(compose).toContain("External outbound is unavailable in v1");
    expect(compose).toContain("Send protected");
    expect(settings).toContain("External inbound");
    expect(settings).toContain("ordinary email");
    expect(settings).toContain("Standard email");
    expect(settings).not.toMatch(/provider|adapter|SMTP|end-to-end encrypted/iu);
  });

  it("Render OSL Mail without labeling SMTP as end-to-end encrypted", () => {
    const external = {
      threadId: "abcdefghijkl",
      retrievalId: "abcdefghijklx",
      expiresAt: 200,
      messages: [{ messageId: "abcdefghijklm", from: "friend@example.com", to: ["liam@oslprivacy.com"], subject: "Hi", body: "Ordinary mail", receivedAt: 100, transit: "externalSmtp" as const }],
    };
    const html = oslMailViewMarkup({
      ...base,
      available: true,
      status: { ...status, unreadCount: 1 },
      threads: [{ threadId: "abcdefghijkl", subject: "Hi", correspondent: "friend@example.com", latestAt: 100, unread: true, transit: "externalSmtp" }],
      activeThread: external,
    });
    expect(html).toContain("Standard email");
    expect(html).toContain("Ordinary email outside OSL protection");
    expect(html).toContain('<span class="osl-mail-transit is-standard-email" title="Ordinary email outside OSL protection">Standard email</span>');
    expect(html.match(/Standard email/gu)).toHaveLength(2);
    expect(html.match(/is-standard-email/gu)).toHaveLength(2);
    expect(html).not.toContain("is-e2ee");
    expect(html).not.toContain("OSL protected");
    expect(html).not.toContain("Protected between verified OSL identities");
    expect(html).not.toMatch(/SMTP|externalSmtp|E2EE|end-to-end|encrypted/iu);
  });

  it("requires device acknowledgment before claiming server deletion", () => {
    const activeThread = { threadId: "abcdefghijkl", retrievalId: "abcdefghijklx", expiresAt: 200, messages: [{ messageId: "abcdefghijklm", from: "friend@oslprivacy.com", to: ["liam@oslprivacy.com"], subject: "Hi", body: "Private", receivedAt: 100, transit: "oslE2ee" as const }] };
    const html = oslMailViewMarkup({ ...base, available: true, status: { ...status, unreadCount: 1 }, activeThread });
    expect(html).toContain("Acknowledge device copy & request server deletion");
    expect(html).not.toContain("Server deletion confirmed");
    expect(html).not.toMatch(VERIFIED_DELETION_CLAIM);
  });

  it("keeps confirmation markup product-facing", () => {
    const activeThread = { threadId: "abcdefghijkl", retrievalId: "abcdefghijklx", expiresAt: 200, messages: [{ messageId: "abcdefghijklm", from: "friend@oslprivacy.com", to: ["liam@oslprivacy.com"], subject: "Hi", body: "Private", receivedAt: 100, transit: "oslE2ee" as const }] };
    const html = oslMailViewMarkup({
      ...base,
      available: true,
      status: { ...status, unreadCount: 1 },
      activeThread,
      deleteReceipt: { retrievalId: "abcdefghijklx", deletedMessageIds: ["abcdefghijklm"], deletedAt: 300, receiptSha256: "a".repeat(64), serverDeleteConfirmed: true },
      burnReceipt: { address: "liam@oslprivacy.com", burnedAt: 400, deletedMessages: 1, receiptSha256: "b".repeat(64), mailboxDisabled: true },
    });
    expect(html).toContain("Server deletion requested");
    expect(html).toContain("Mailbox burn requested");
    expect(html).not.toMatch(/keyservers?|ratchets?|receipts?|browser profiles?|provider adapters?/iu);
  });

  it("never displays a requested deletion as a verified deletion", () => {
    // action: perform the forbidden operation -- render BOTH destructive
    // receipts, on every pane, in the most favourable shape the adapter allows
    // (serverDeleteConfirmed and mailboxDisabled are literal `true`).
    const activeThread = { threadId: "abcdefghijkl", retrievalId: "abcdefghijklx", expiresAt: 200, messages: [{ messageId: "abcdefghijklm", from: "friend@oslprivacy.com", to: ["liam@oslprivacy.com"], subject: "Hi", body: "Private", receivedAt: 100, transit: "oslE2ee" as const }] };
    const model: OslMailViewModel = {
      ...base,
      available: true,
      status: { ...status, unreadCount: 1 },
      activeThread,
      deleteReceipt: { retrievalId: "abcdefghijklx", deletedMessageIds: ["abcdefghijklm"], deletedAt: 300, receiptSha256: "a".repeat(64), serverDeleteConfirmed: true },
      burnReceipt: { address: "liam@oslprivacy.com", burnedAt: 400, deletedMessages: 7, receiptSha256: "b".repeat(64), mailboxDisabled: true },
    };

    for (const pane of ["inbox", "compose", "settings"] as const) {
      const html = oslMailViewMarkup({ ...model, pane });

      // must_not_change: the prohibited effect never occurs on any pane.
      expect(html).not.toMatch(VERIFIED_DELETION_CLAIM);
      expect(html).not.toContain("Server deletion confirmed");
      expect(html).not.toContain("Mailbox burn confirmed");

      // must_change: the UI states the true limitation, and states it beside
      // the outcome rather than burying it elsewhere in the page.
      expect(html).toContain(OSL_MAIL_DELETION_UNVERIFIED_NOTE);
      expect(html).toContain("Server deletion requested");
      expect(html).toContain("Mailbox burn requested");

      // The digest is OSL's own sha256 of a local timestamp plus a local
      // request id.  It must never stand unlabelled beside an outcome, where it
      // reads as remote evidence of destruction.
      expect(html).toContain("OSL reference aaaaaaaaaaaa…");
      expect(html).toContain("OSL reference bbbbbbbbbbbb…");
      expect(html).not.toMatch(/<code>[a-f0-9]{12}…<\/code>/u);

      // A server-reported count is reported as the server's claim, never as a
      // verified erasure total.
      expect(html).toContain("The server reported the address disabled and 7 messages deleted.");
    }
  });

  it("states the retrieval limitation in settings rather than promising confirmation", () => {
    const html = oslMailViewMarkup({ ...base, available: true, status, pane: "settings" });
    expect(html).toContain("before OSL requests server deletion");
    expect(html).toContain("OSL cannot verify that a remote copy is gone");
    expect(html).not.toContain("before server deletion is confirmed");
    expect(html).not.toMatch(VERIFIED_DELETION_CLAIM);
  });
});
