import { describe, expect, it } from "vitest";
import { parseLanSession, parseLanSync, parseSharedDocument } from "./osl-collab";
const document = { kind: "document", title: "Plan", body: "Private", folder: "Team", tags: ["lan"], favorite: false };
describe("local-first collaboration IPC", () => {
  it("strictly accepts a no-cloud encrypted LAN host receipt", () => { const invitation = { code: "osl-lan-v1|192.168.1.4:4000|" + "a".repeat(32) + "|" + "b".repeat(64), address: "192.168.1.4:4000", roomId: "a".repeat(32), encrypted: true, requiresCloud: false, requiresPro: false }; expect(parseLanSession({ sessionId: "a".repeat(32), role: "host", revision: 0, document, invitation, connected: true, encrypted: true, cloud: false })?.invitation).toEqual(invitation); });
  it("rejects hidden cloud or extra document fields", () => { expect(parseSharedDocument({ ...document, cloudId: "remote" })).toBeNull(); expect(parseLanSession({ sessionId: "a".repeat(32), role: "guest", revision: 0, document, invitation: null, connected: true, encrypted: true, cloud: true })).toBeNull(); });
  it("preserves explicit conflicts", () => { expect(parseLanSync({ revision: 4, document, changed: true, conflict: true, connected: true })?.conflict).toBe(true); });
});
