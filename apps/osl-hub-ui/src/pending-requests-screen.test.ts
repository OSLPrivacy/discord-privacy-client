import { describe, expect, it } from "vitest";

import { PENDING_REQUESTS_FIXTURE } from "./pending-requests-screen-data";
import {
  acceptRequest,
  declineRequest,
  renderPendingRequestsScreen,
} from "./pending-requests-screen";

describe("TASK 0228 pending request actions connect to commands", () => {
  it("accepting a fixture request moves it from Pending to All", () => {
    const requestId = PENDING_REQUESTS_FIXTURE.pending[0].id;
    console.log(`TASK0228_START pending=${PENDING_REQUESTS_FIXTURE.pending.length} all=${PENDING_REQUESTS_FIXTURE.all.length}`);

    const beforePending = renderPendingRequestsScreen(PENDING_REQUESTS_FIXTURE, "pending");
    expect(beforePending).toContain(`data-request-id="${requestId}"`);
    expect(beforePending).toContain('data-request-action="accept"');
    expect(beforePending).toContain('data-request-action="decline"');

    const afterAccept = acceptRequest(PENDING_REQUESTS_FIXTURE, requestId);

    expect(afterAccept.pending).toHaveLength(0);
    expect(afterAccept.all).toHaveLength(1);
    expect(afterAccept.all[0].id).toBe(requestId);

    const pendingTab = renderPendingRequestsScreen(afterAccept, "pending");
    const allTab = renderPendingRequestsScreen(afterAccept, "all");

    expect(pendingTab).not.toContain(`data-request-id="${requestId}"`);
    expect(allTab).toContain(`data-request-id="${requestId}"`);

    console.log(
      `TASK0228_ACCEPT request_id=${requestId} pending_count=${afterAccept.pending.length} all_count=${afterAccept.all.length}`,
    );
    console.log(`TASK0228_DONE moved_pending_to_all=${afterAccept.all.length === 1 && afterAccept.pending.length === 0}`);
  });

  it("declining a fixture request removes it from Pending without adding it to All", () => {
    const requestId = PENDING_REQUESTS_FIXTURE.pending[0].id;

    const afterDecline = declineRequest(PENDING_REQUESTS_FIXTURE, requestId);

    expect(afterDecline.pending).toHaveLength(0);
    expect(afterDecline.all).toHaveLength(0);

    const pendingTab = renderPendingRequestsScreen(afterDecline, "pending");
    const allTab = renderPendingRequestsScreen(afterDecline, "all");
    expect(pendingTab).not.toContain(`data-request-id="${requestId}"`);
    expect(allTab).not.toContain(`data-request-id="${requestId}"`);

    console.log(
      `TASK0228_DECLINE request_id=${requestId} pending_count=${afterDecline.pending.length} all_count=${afterDecline.all.length}`,
    );
  });
});
