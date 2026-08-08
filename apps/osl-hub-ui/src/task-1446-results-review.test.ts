import { describe, expect, it } from "vitest";
import {
  flattenReviewResultStoreOutput,
  resultsReviewScreenMarkup,
  ResultsReviewSession,
  type ReviewResultStoreOutput,
} from "./results-review";

/** Same shape and account ids as the TASK 1444 direct-command fixture output. */
const FIXTURE: ReviewResultStoreOutput = {
  totalMatches: 3,
  groups: [
    {
      accountId: "discord-account-alpha-1444",
      matches: [
        {
          fullText: "password: correct horse battery staple",
          reason: "This looks like a password, API key, or access credential.",
          service: "Discord",
          place: "dm:task-1444-alpha",
          date: "2026-08-06",
          time: "09:30",
        },
      ],
    },
    {
      accountId: "telegram-account-beta-1444",
      matches: [
        {
          fullText: "recovery phrase: maple bridge cloud midnight",
          reason: "This may expose account or wallet recovery material.",
          service: "Telegram",
          place: "chat:task-1444-beta",
          date: "2026-08-06",
          time: "10:45",
        },
        {
          fullText: "card: 4111 1111 1111 1111",
          reason: "This looks like a payment card number.",
          service: "Telegram",
          place: "chat:task-1444-beta",
          date: "2026-08-06",
          time: "10:46",
        },
      ],
    },
  ],
};

describe("TASK 1446 connect results review", () => {
  it("opens the first result automatically and flattens groups in order", () => {
    const items = flattenReviewResultStoreOutput(FIXTURE);
    console.log(`task_1446_flattened_count=${items.length}`);
    expect(items).toHaveLength(3);
    const session = new ResultsReviewSession(FIXTURE);
    expect(session.currentItem()?.match.fullText).toBe("password: correct horse battery staple");
    expect(session.remainingCount()).toBe(3);
  });

  it("Keep removes one result from this review and Review next opens the next one", () => {
    const session = new ResultsReviewSession(FIXTURE);
    const before = session.remainingCount();
    session.keep();
    const afterKeep = session.remainingCount();
    console.log(`task_1446_before_keep=${before} after_keep=${afterKeep} current_after_keep=${session.currentItem()}`);
    expect(afterKeep).toBe(before - 1);
    expect(session.currentItem()).toBeNull();

    const opened = session.reviewNext();
    console.log(`task_1446_review_next_opened=${opened?.match.fullText}`);
    expect(opened?.match.fullText).toBe("recovery phrase: maple bridge cloud midnight");
    expect(session.currentItem()?.match.fullText).toBe("recovery phrase: maple bridge cloud midnight");
  });

  it("Mark for deletion records the decision and clears the slot until Review next", () => {
    const session = new ResultsReviewSession(FIXTURE);
    session.markForDeletion();
    expect(session.remainingCount()).toBe(2);
    expect(session.currentItem()).toBeNull();
    const next = session.reviewNext();
    expect(next?.match.fullText).toBe("recovery phrase: maple bridge cloud midnight");
    const summary = session.snapshot().accountSummaries.find((row) => row.accountId === "discord-account-alpha-1444");
    console.log(`task_1446_marked_summary=${JSON.stringify(summary)}`);
    expect(summary?.markedForDeletionCount).toBe(1);
    expect(summary?.pendingCount).toBe(0);
  });

  it("Finish review ends the session even with results still pending", () => {
    const session = new ResultsReviewSession(FIXTURE);
    session.keep();
    const snapshot = session.finishReview();
    console.log(`task_1446_finish_snapshot=${JSON.stringify(snapshot)}`);
    expect(snapshot.finished).toBe(true);
    expect(snapshot.currentItem).toBeNull();
    expect(snapshot.remainingCount).toBe(2);
    expect(snapshot.reviewedCount).toBe(1);
    expect(() => session.keep()).toThrow();
    expect(session.reviewNext()).toBeNull();
  });

  it("builds account summaries across kept, marked, and pending results", () => {
    const session = new ResultsReviewSession(FIXTURE);
    session.keep();
    session.reviewNext();
    session.markForDeletion();
    const snapshot = session.snapshot();
    const byAccount = Object.fromEntries(snapshot.accountSummaries.map((row) => [row.accountId, row]));
    console.log(`task_1446_account_summaries=${JSON.stringify(byAccount)}`);
    expect(byAccount["discord-account-alpha-1444"]).toEqual({
      accountId: "discord-account-alpha-1444",
      totalCount: 1,
      keptCount: 1,
      markedForDeletionCount: 0,
      pendingCount: 0,
    });
    expect(byAccount["telegram-account-beta-1444"]).toEqual({
      accountId: "telegram-account-beta-1444",
      totalCount: 2,
      keptCount: 0,
      markedForDeletionCount: 1,
      pendingCount: 1,
    });
  });

  it("renders Keep, Mark for deletion, Review next, and Finish review on the markup", () => {
    const session = new ResultsReviewSession(FIXTURE);
    const openMarkup = resultsReviewScreenMarkup(session.snapshot());
    console.log(`task_1446_open_markup_has_keep=${openMarkup.includes('data-results-review-action="keep"')} has_mark=${openMarkup.includes('data-results-review-action="mark-for-deletion"')} has_finish=${openMarkup.includes('data-results-review-action="finish-review"')}`);
    expect(openMarkup).toContain('data-results-review-action="keep"');
    expect(openMarkup).toContain('data-results-review-action="mark-for-deletion"');
    expect(openMarkup).toContain('data-results-review-action="finish-review"');
    expect(openMarkup).toContain("discord-account-alpha-1444");

    session.keep();
    const closedMarkup = resultsReviewScreenMarkup(session.snapshot());
    console.log(`task_1446_closed_markup_has_review_next=${closedMarkup.includes('data-results-review-action="review-next"')}`);
    expect(closedMarkup).toContain('data-results-review-action="review-next"');
    expect(closedMarkup).not.toContain('data-results-review-action="keep"');
  });
});
