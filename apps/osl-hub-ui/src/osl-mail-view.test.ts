import { describe, expect, it } from "vitest";
import { oslMailViewMarkup, type OslMailViewModel } from "./osl-mail-view";

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
  });

  it("renders external SMTP mail without end-to-end encrypted labels", () => {
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
    expect(html).not.toMatch(/SMTP|E2EE|end-to-end|encrypted/iu);
  });

  it("requires device acknowledgment before claiming server deletion", () => {
    const activeThread = { threadId: "abcdefghijkl", retrievalId: "abcdefghijklx", expiresAt: 200, messages: [{ messageId: "abcdefghijklm", from: "friend@oslprivacy.com", to: ["liam@oslprivacy.com"], subject: "Hi", body: "Private", receivedAt: 100, transit: "oslE2ee" as const }] };
    const html = oslMailViewMarkup({ ...base, available: true, status: { ...status, unreadCount: 1 }, activeThread });
    expect(html).toContain("Acknowledge device copy & delete server copy");
    expect(html).not.toContain("Server deletion confirmed");
  });
});
