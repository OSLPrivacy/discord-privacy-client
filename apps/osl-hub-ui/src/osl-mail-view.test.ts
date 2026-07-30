import { describe, expect, it } from "vitest";
import { oslMailViewMarkup, type OslMailViewModel } from "./osl-mail-view";

const base: OslMailViewModel = { loading: false, available: false, signedUsername: "liam", status: null, threads: [], activeThread: null, pane: "inbox", notifications: true, deleteReceipt: null, sendReceipt: null, burnReceipt: null, error: null };

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

  it("keeps external outbound unavailable and labels external inbound as SMTP", () => {
    const status = { available: true as const, provisioned: true as const, address: "liam@oslprivacy.com" as const, unreadCount: 0, retentionSeconds: 3600 };
    const compose = oslMailViewMarkup({ ...base, available: true, status, pane: "compose" });
    const settings = oslMailViewMarkup({ ...base, available: true, status, pane: "settings" });
    expect(compose).toContain("External outbound is unavailable in v1");
    expect(compose).toContain("Send E2EE");
    expect(settings).toContain("External inbound");
    expect(settings).toContain("ordinary SMTP");
  });

  it("requires device acknowledgment before claiming server deletion", () => {
    const status = { available: true as const, provisioned: true as const, address: "liam@oslprivacy.com" as const, unreadCount: 1, retentionSeconds: 3600 };
    const activeThread = { threadId: "abcdefghijkl", retrievalId: "abcdefghijklx", expiresAt: 200, messages: [{ messageId: "abcdefghijklm", from: "friend@oslprivacy.com", to: ["liam@oslprivacy.com"], subject: "Hi", body: "Private", receivedAt: 100, transit: "oslE2ee" as const }] };
    const html = oslMailViewMarkup({ ...base, available: true, status, activeThread });
    expect(html).toContain("Acknowledge device copy & delete server copy");
    expect(html).not.toContain("Server deletion confirmed");
  });
});
