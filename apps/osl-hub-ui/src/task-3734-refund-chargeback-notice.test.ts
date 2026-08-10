import { describe, expect, it } from "vitest";

import {
  ACCOUNT_BUYER_NOTICES,
  accountScreenState,
  closeAccountBuyerNotice,
  renderAccountScreen,
  type AccountBuyerNoticeEvent,
} from "./account-screen";
import { ACCOUNT_SCREEN_SECRETS, ACCOUNT_SCREEN_SETTINGS } from "./account-screen-data";
import { pageTitle, visibleText } from "./settings-home-words";

const events: readonly AccountBuyerNoticeEvent[] = ["refund", "chargeback"];

function exactCount(haystack: string, needle: string): number {
  return haystack.split(needle).length - 1;
}

describe("TASK 3734 refund and chargeback Account notices", () => {
  it.each(events)("renders the exact %s notice and both controls without naming the other event", (event) => {
    const otherEvent = event === "refund" ? "chargeback" : "refund";
    const state = accountScreenState(ACCOUNT_SCREEN_SECRETS, ACCOUNT_SCREEN_SETTINGS, event);
    const markup = renderAccountScreen(state);
    const screenTreeText = visibleText(markup);

    expect(pageTitle(markup)).toBe("Account");
    expect(markup).toContain(`data-account-buyer-event="${event}"`);
    expect(exactCount(screenTreeText, ACCOUNT_BUYER_NOTICES[event].notice)).toBe(1);
    expect(exactCount(screenTreeText, "Contact support")).toBe(1);
    expect(exactCount(screenTreeText, "Close")).toBe(1);
    expect(screenTreeText).toContain(ACCOUNT_BUYER_NOTICES[event].title);
    expect(screenTreeText).not.toContain(ACCOUNT_BUYER_NOTICES[otherEvent].title);
    expect(screenTreeText).not.toContain(ACCOUNT_BUYER_NOTICES[otherEvent].notice);

    console.info(`TASK3734_RENDER_${event.toUpperCase()}_TITLE=${pageTitle(markup)}`);
    console.info(`TASK3734_RENDER_${event.toUpperCase()}_NOTICE=${ACCOUNT_BUYER_NOTICES[event].notice}`);
    console.info(`TASK3734_RENDER_${event.toUpperCase()}_CONTROLS=Contact support|Close`);
    console.info(`TASK3734_RENDER_${event.toUpperCase()}_OTHER_EVENT_COUNT=0`);
  });

  it("closes the current buyer notice without changing the Account view", () => {
    for (const event of events) {
      const state = accountScreenState(ACCOUNT_SCREEN_SECRETS, ACCOUNT_SCREEN_SETTINGS, event);
      const closed = closeAccountBuyerNotice(state);
      expect(closed.buyerNoticeEvent).toBeNull();
      expect(closed.saved).toBe(state.saved);
      expect(closed.draft).toBe(state.draft);
      expect(renderAccountScreen(closed)).not.toContain("data-account-buyer-event");
    }
  });
});
