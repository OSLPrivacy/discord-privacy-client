import { describe, expect, it, vi } from "vitest";

import { PENDING_REQUESTS_FIXTURE } from "./pending-requests-screen-data";
import { addPendingRequest } from "./pending-requests-screen";
import {
  pendingRequestFromRedeemedInvite,
  runCreateInviteLink,
  runPasteInviteLink,
  type InviteLinkActionsModel,
  type RedeemedInviteLinkRequest,
} from "./invite-link-actions";

const FIXTURE_LINK = "https://invite.osl.local/one-use/N2mI-dRDm7RQEQ4ywSzIZg.ujrDhhQJfGFdkiBLkHxv6tDuL4Pxp4nyj1z6WecEHX4";

describe("TASK 0221 connect invite-link actions to their commands", () => {
  it("pasting a fixture link adds one request to Pending", async () => {
    const redeemed: RedeemedInviteLinkRequest = {
      peerDiscordId: "900000000000022802",
      scopeStorageKey: "dm:900000000000022802",
      createdAtUnixSeconds: 1_700_000_000,
    };
    const redeemInviteLink = vi.fn().mockResolvedValue(redeemed);

    const before = PENDING_REQUESTS_FIXTURE;
    console.log(`TASK0221_PENDING_BEFORE=${before.pending.length}`);

    const request = await runPasteInviteLink(FIXTURE_LINK, redeemInviteLink);

    expect(redeemInviteLink).toHaveBeenCalledTimes(1);
    expect(redeemInviteLink).toHaveBeenCalledWith(FIXTURE_LINK);
    expect(request).toEqual(pendingRequestFromRedeemedInvite(redeemed));

    const after = addPendingRequest(before, request);

    console.log(`TASK0221_PENDING_AFTER=${after.pending.length}`);
    console.log(`TASK0221_ADDED_ONE=${after.pending.length === before.pending.length + 1}`);

    expect(after.pending).toHaveLength(before.pending.length + 1);
    expect(after.pending.some((entry) => entry.discordId === redeemed.peerDiscordId)).toBe(true);
    // The original fixture state is untouched -- this is a new state, not a mutation.
    expect(before.pending).toHaveLength(1);
  });

  it("pasting the same link twice does not duplicate the request in Pending", async () => {
    const redeemed: RedeemedInviteLinkRequest = {
      peerDiscordId: "900000000000022803",
      scopeStorageKey: "dm:900000000000022803",
      createdAtUnixSeconds: 1_700_000_001,
    };
    const redeemInviteLink = vi.fn().mockResolvedValue(redeemed);

    const request = await runPasteInviteLink(FIXTURE_LINK, redeemInviteLink);
    const onceAdded = addPendingRequest(PENDING_REQUESTS_FIXTURE, request);
    const twiceAdded = addPendingRequest(onceAdded, request);

    expect(twiceAdded.pending).toHaveLength(onceAdded.pending.length);
  });

  it("blank paste input never reaches the redeem command", async () => {
    const redeemInviteLink = vi.fn();
    await expect(runPasteInviteLink("   ", redeemInviteLink)).rejects.toThrow(/paste an invite link/);
    expect(redeemInviteLink).not.toHaveBeenCalled();
  });

  it("a failed redeem (an already-consumed link) adds nothing to Pending", async () => {
    const redeemInviteLink = vi.fn().mockRejectedValue(new Error("OSL: invite link is already consumed"));

    await expect(runPasteInviteLink(FIXTURE_LINK, redeemInviteLink)).rejects.toThrow("OSL: invite link is already consumed");

    console.log(`TASK0221_CONSUMED_LINK_PENDING_UNCHANGED=${PENDING_REQUESTS_FIXTURE.pending.length === 1}`);
    expect(PENDING_REQUESTS_FIXTURE.pending).toHaveLength(1);
  });

  it("create asks the backend for a fresh link and replaces the model's link, clearing the in-flight flag", async () => {
    const created = {
      inviteId: "N2mI-dRDm7RQEQ4ywSzIZg",
      link: FIXTURE_LINK,
      recipientLabel: "No OSL name 0221",
      expiresAt: 1_700_086_400,
    };
    const createOneUseInviteLink = vi.fn().mockResolvedValue(created);
    const model: InviteLinkActionsModel = { link: null, creating: true, pasteValue: "" };

    const next = await runCreateInviteLink(model, createOneUseInviteLink);

    expect(createOneUseInviteLink).toHaveBeenCalledTimes(1);
    expect(next.link).toEqual(created);
    expect(next.creating).toBe(false);
    console.log(`TASK0221_CREATE_LINK=${next.link?.link}`);
  });
});
