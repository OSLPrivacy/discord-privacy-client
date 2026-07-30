import { describe, expect, it } from "vitest";
import { parseCircleAudience, parseLanSession, parseLanSync, parseSharedDocument } from "./osl-collab";
const document = { kind: "document", title: "Plan", body: "Private", folder: "Team", tags: ["lan"], favorite: false };
const circleAudience = { audienceId: "c".repeat(32), name: "Close friends", memberCount: 2, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }, { memberId: "2".repeat(32), name: "Theo", verified: false }], consentGranted: true, boundToCurrentCircle: true, postingAuthorized: true };
describe("local-first collaboration IPC", () => {
  it("strictly accepts a no-cloud encrypted LAN host receipt", () => { const invitation = { code: "osl-lan-v1|192.168.1.4:4000|" + "a".repeat(32) + "|" + "b".repeat(64), address: "192.168.1.4:4000", roomId: "a".repeat(32), encrypted: true, requiresCloud: false, requiresPro: false }; expect(parseLanSession({ sessionId: "a".repeat(32), role: "host", revision: 0, document, invitation, connected: true, encrypted: true, cloud: false })?.invitation).toEqual(invitation); });
  it("rejects hidden cloud or extra document fields", () => { expect(parseSharedDocument({ ...document, cloudId: "remote" })).toBeNull(); expect(parseLanSession({ sessionId: "a".repeat(32), role: "guest", revision: 0, document, invitation: null, connected: true, encrypted: true, cloud: true })).toBeNull(); });
  it("preserves explicit conflicts", () => { expect(parseLanSync({ revision: 4, document, changed: true, conflict: true, connected: true })?.conflict).toBe(true); });
});

describe("Circle audience contract", () => {
  it("models a visible private audience without exposing service machinery", () => {
    expect(parseCircleAudience(circleAudience)).toEqual({ audienceId: "c".repeat(32), name: "Close friends", memberCount: 2, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }, { memberId: "2".repeat(32), name: "Theo", verified: false }], canPost: true, refusal: null });
  });

  it("refuses posting when consent, binding, or authority is absent", () => {
    expect(parseCircleAudience({ ...circleAudience, consentGranted: false })?.refusal).toBe("consent");
    expect(parseCircleAudience({ ...circleAudience, boundToCurrentCircle: false })?.refusal).toBe("binding");
    expect(parseCircleAudience({ ...circleAudience, postingAuthorized: false })?.refusal).toBe("authority");
    expect(parseCircleAudience({ ...circleAudience, postingAuthorized: undefined })).toBeNull();
  });

  it("keeps hidden or count-only membership from carrying visible people", () => {
    expect(parseCircleAudience({ ...circleAudience, membershipVisibility: "count-only", visibleMembers: [] })?.canPost).toBe(true);
    expect(parseCircleAudience({ ...circleAudience, membershipVisibility: "hidden", visibleMembers: [] })?.membershipVisibility).toBe("hidden");
    expect(parseCircleAudience({ ...circleAudience, membershipVisibility: "hidden" })).toBeNull();
  });

  it("bounds audience and member labels without treating names as service handles", () => {
    expect(parseCircleAudience({ ...circleAudience, name: "" })).toBeNull();
    expect(parseCircleAudience({ ...circleAudience, visibleMembers: [{ memberId: "1".repeat(32), name: "@maya", verified: true }] })?.visibleMembers[0]?.name).toBe("@maya");
    expect(parseCircleAudience({ ...circleAudience, visibleMembers: [{ memberId: "1".repeat(32), name: "Maya\u0000", verified: true }] })).toBeNull();
  });
});
