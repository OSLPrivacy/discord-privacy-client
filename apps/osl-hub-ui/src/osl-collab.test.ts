import { describe, expect, it } from "vitest";
import { composeEnclavePost, createEnclaveServerContent, parseEnclaveAudience, parseLanSession, parseLanSync, parseSharedDocument, type EnclaveServerContentState } from "./osl-collab";
const document = { kind: "document", title: "Plan", body: "Private", folder: "Team", tags: ["lan"], favorite: false };
const enclaveAudience = { audienceId: "c".repeat(32), name: "Close friends", memberCount: 2, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }, { memberId: "2".repeat(32), name: "Theo", verified: false }], consentGranted: true, boundToCurrentEnclave: true, postingAuthorized: true };
describe("local-first collaboration IPC", () => {
  it("strictly accepts a no-cloud encrypted LAN host receipt", () => { const invitation = { code: "osl-lan-v1|192.168.1.4:4000|" + "a".repeat(32) + "|" + "b".repeat(64), address: "192.168.1.4:4000", roomId: "a".repeat(32), encrypted: true, requiresCloud: false, requiresPro: false }; expect(parseLanSession({ sessionId: "a".repeat(32), role: "host", revision: 0, document, invitation, connected: true, encrypted: true, cloud: false })?.invitation).toEqual(invitation); });
  it("rejects hidden cloud or extra document fields", () => { expect(parseSharedDocument({ ...document, cloudId: "remote" })).toBeNull(); expect(parseLanSession({ sessionId: "a".repeat(32), role: "guest", revision: 0, document, invitation: null, connected: true, encrypted: true, cloud: true })).toBeNull(); });
  it("preserves explicit conflicts", () => { expect(parseLanSync({ revision: 4, document, changed: true, conflict: true, connected: true })?.conflict).toBe(true); });
});

describe("Enclave audience contract", () => {
  it("models a visible private audience without exposing service machinery", () => {
    expect(parseEnclaveAudience(enclaveAudience)).toEqual({ audienceId: "c".repeat(32), name: "Close friends", memberCount: 2, membershipVisibility: "visible", visibleMembers: [{ memberId: "1".repeat(32), name: "Maya", verified: true }, { memberId: "2".repeat(32), name: "Theo", verified: false }], canPost: true, refusal: null });
  });

  it("refuses posting when consent, binding, or authority is absent", () => {
    expect(parseEnclaveAudience({ ...enclaveAudience, consentGranted: false })?.refusal).toBe("consent");
    expect(parseEnclaveAudience({ ...enclaveAudience, boundToCurrentEnclave: false })?.refusal).toBe("binding");
    expect(parseEnclaveAudience({ ...enclaveAudience, postingAuthorized: false })?.refusal).toBe("authority");
    expect(parseEnclaveAudience({ ...enclaveAudience, postingAuthorized: undefined })).toBeNull();
  });

  it("keeps hidden or count-only membership from carrying visible people", () => {
    expect(parseEnclaveAudience({ ...enclaveAudience, membershipVisibility: "count-only", visibleMembers: [] })?.canPost).toBe(true);
    expect(parseEnclaveAudience({ ...enclaveAudience, membershipVisibility: "hidden", visibleMembers: [] })?.membershipVisibility).toBe("hidden");
    expect(parseEnclaveAudience({ ...enclaveAudience, membershipVisibility: "hidden" })).toBeNull();
  });

  it("bounds audience and member labels without treating names as service handles", () => {
    expect(parseEnclaveAudience({ ...enclaveAudience, name: "" })).toBeNull();
    expect(parseEnclaveAudience({ ...enclaveAudience, visibleMembers: [{ memberId: "1".repeat(32), name: "@maya", verified: true }] })?.visibleMembers[0]?.name).toBe("@maya");
    expect(parseEnclaveAudience({ ...enclaveAudience, visibleMembers: [{ memberId: "1".repeat(32), name: "Maya\u0000", verified: true }] })).toBeNull();
  });

  it("Compose Enclave posts with audience membership shown before posting", () => {
    const composition = composeEnclavePost(enclaveAudience, " Dinner is at 7.\nBring notes. ");

    expect(composition.status).toBe("ready");
    expect(composition).toMatchObject({
      audienceId: "c".repeat(32),
      audienceName: "Close friends",
      body: "Dinner is at 7.\nBring notes.",
      encryptedForAudience: true,
      feedOrder: "chronological",
      sendAuthority: "user-action-required",
      membershipReview: {
        audienceId: "c".repeat(32),
        audienceName: "Close friends",
        memberCount: 2,
        shownBeforePosting: true,
      },
    });
    expect(composition.membershipReview?.members.map((member) => member.name)).toEqual(["Maya", "Theo"]);
    expect(composeEnclavePost({ ...enclaveAudience, membershipVisibility: "count-only", visibleMembers: [] }, "Dinner is at 7.")).toMatchObject({ status: "refused", reason: "membership-review", encryptedForAudience: false, sendAuthority: "none", membershipReview: null });
    expect(composeEnclavePost({ ...enclaveAudience, memberCount: 3 }, "Dinner is at 7.")).toMatchObject({ status: "refused", reason: "membership-review" });
    expect(composeEnclavePost({ ...enclaveAudience, consentGranted: false }, "Dinner is at 7.")).toMatchObject({ status: "refused", reason: "consent", membershipReview: expect.objectContaining({ shownBeforePosting: true }) });
    expect(composeEnclavePost(enclaveAudience, " \n\t ")).toMatchObject({ status: "refused", reason: "draft", membershipReview: expect.objectContaining({ shownBeforePosting: true }) });
  });

  it("TASK 1319 refuses server content when only the author changes to a non-member", () => {
    const ava = "a".repeat(32);
    const cy = "c".repeat(32);
    const state: EnclaveServerContentState = {
      serverId: "5".repeat(32),
      members: [{ memberId: ava, name: "Ava" }],
      content: [],
    };
    const maplePost = { contentId: "maple-post", authorMemberId: ava, body: "MAPLE-4172" };

    console.log(`TASK 1319 before server_content_count=${state.content.length}`);
    expect(state.content).toHaveLength(0);

    const avaCreate = createEnclaveServerContent(state, maplePost);
    expect(avaCreate).toEqual({ status: "accepted", contentId: "maple-post", serverContentCount: 1 });
    const stored = state.content.find((item) => item.contentId === "maple-post");
    console.log(`TASK 1319 after_ava server_content_count=${state.content.length} maple-post=${stored?.body ?? ""}`);
    expect(stored?.body).toBe("MAPLE-4172");
    expect(state.content).toHaveLength(1);

    const cyCreate = createEnclaveServerContent(state, { ...maplePost, authorMemberId: cy });
    console.log(`TASK 1319 cy status=${cyCreate.status} reason=${cyCreate.status === "refused" ? cyCreate.reason : ""} member=Cy server_content_count=${state.content.length}`);
    expect(cyCreate).toEqual({ status: "refused", reason: "not-server-member", memberId: cy, serverContentCount: 1 });

    const retained = state.content.find((item) => item.contentId === "maple-post");
    console.log(`TASK 1319 final server_content_count=${state.content.length} maple-post=${retained?.body ?? ""}`);
    expect(retained?.body).toBe("MAPLE-4172");
    expect(state.content).toHaveLength(1);
  });
});
