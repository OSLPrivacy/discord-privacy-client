/**
 * Fixture data for the pending-requests screen: one incoming friend request,
 * used by the screen's own tests and by anything that wants a known starting
 * point without a live backend.
 */

import type { PendingRequestsState } from "./pending-requests-screen";

export const PENDING_REQUESTS_FIXTURE: PendingRequestsState = {
  pending: [{ id: "REQ-0228", alias: "Pat", discordId: "900000000000022801" }],
  all: [],
};
