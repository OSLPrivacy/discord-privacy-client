import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import type { ReviewResultStoreOutput } from "./results-review";
import {
  CANCELLED_LINE,
  CANCEL_DELETE_CONTROL,
  CANCEL_DELETE_LABEL,
  CONFIRM_DELETE_CONTROL,
  CONFIRM_DELETE_LABEL,
  DELETE_MARKED_CONTROL,
  DELETE_MARKED_LABEL,
  ProMarkedDeletionReview,
  ReviewDecisionRecorder,
  type MarkedDeletionCommandPort,
  type MarkedDeletionCount,
  type MarkedDeletionReply,
  type ReviewedMessage,
} from "./pro-marked-deletion-review";

/**
 * The replies below are not written here: they are what
 * `run_pro_marked_deletion_command` (TASK 1449) actually answered, recorded by
 * apps/osl-hub-ui/scripts/task-1450-record-backend.mjs. The replay is keyed by
 * the request, so a request this screen builds that the command never answered
 * has no reply and fails the check rather than being quietly imitated.
 */
interface RecordedExchange {
  command: "count" | "delete";
  request: Record<string, unknown>;
  reply: MarkedDeletionReply;
}

interface BackendTranscript {
  fixtureMessages: ReviewedMessage[];
  exchanges: RecordedExchange[];
}

const transcript: BackendTranscript = JSON.parse(
  readFileSync(new URL("./task-1450-backend-transcript.json", import.meta.url), "utf8"),
);

function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>).sort(([a], [b]) =>
      a < b ? -1 : a > b ? 1 : 0,
    );
    return `{${entries.map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`).join(",")}}`;
  }
  return JSON.stringify(value);
}

interface ReplayPort {
  port: MarkedDeletionCommandPort;
  calls: { command: string; request: unknown }[];
}

function replayPort(): ReplayPort {
  const replies = new Map<string, MarkedDeletionReply>();
  for (const exchange of transcript.exchanges) {
    replies.set(`${exchange.command} ${canonical(exchange.request)}`, exchange.reply);
  }
  const calls: { command: string; request: unknown }[] = [];
  const port: MarkedDeletionCommandPort = (command, request) => {
    calls.push({ command, request });
    const reply = replies.get(`${command} ${canonical(request)}`);
    if (!reply) {
      throw new Error(
        `the marked deletion command was never asked this: ${command} ${canonical(request)}`,
      );
    }
    return reply;
  };
  return { port, calls };
}

/** The TASK 1444 grouped, jump-link-free result shape the review reads. */
const REVIEW_RESULTS: ReviewResultStoreOutput = {
  totalMatches: 4,
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
        {
          fullText: "see you at the usual place on Thursday",
          reason: "This mentions a place and a time.",
          service: "Discord",
          place: "dm:task-1444-alpha",
          date: "2026-08-06",
          time: "09:31",
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
          fullText: "my landlord's number is on the fridge",
          reason: "This may identify someone else.",
          service: "Telegram",
          place: "chat:task-1444-beta",
          date: "2026-08-06",
          time: "10:46",
        },
      ],
    },
  ],
};

/**
 * Runs the TASK 1446 review the way a person would: mark the first, keep the
 * second, mark the third, and leave the fourth unopened.
 */
function reviewedMessages(): ReviewedMessage[] {
  const recorder = new ReviewDecisionRecorder(REVIEW_RESULTS);
  recorder.markForDeletion();
  recorder.reviewNext();
  recorder.keep();
  recorder.reviewNext();
  recorder.markForDeletion();
  recorder.reviewNext();
  recorder.finishReview();
  return recorder.reviewedMessages();
}

function screen(plan: "free" | "pro"): { review: ProMarkedDeletionReview; replay: ReplayPort } {
  const replay = replayPort();
  const review = new ProMarkedDeletionReview({
    plan,
    messages: reviewedMessages(),
    port: replay.port,
  });
  return { review, replay };
}

describe("TASK 1450 - Pro deletion review", () => {
  it("hands the review's own decisions to the command, unchanged", () => {
    expect(reviewedMessages()).toEqual(transcript.fixtureMessages);
    expect(reviewedMessages()).toHaveLength(4);
    expect(reviewedMessages().filter((message) => message.decision === "markedForDeletion")).toHaveLength(2);
  });

  it("gives Free no deletion control anywhere on the screen", () => {
    const { review, replay } = screen("free");

    expect(review.controls()).toEqual([]);
    expect(review.hasControl(DELETE_MARKED_CONTROL)).toBe(false);
    expect(review.hasControl(CONFIRM_DELETE_CONTROL)).toBe(false);
    expect(review.hasControl(CANCEL_DELETE_CONTROL)).toBe(false);

    const markup = review.markup();
    expect(markup).not.toContain("<button");
    expect(markup).not.toContain("data-marked-deletion-action");
    expect(markup).not.toContain(DELETE_MARKED_LABEL);
    expect(markup).not.toContain(CONFIRM_DELETE_LABEL);

    expect(() => review.press(DELETE_MARKED_CONTROL)).toThrow(/not on this screen/);
    expect(() => review.press(CONFIRM_DELETE_CONTROL)).toThrow(/not on this screen/);
    expect(replay.calls).toEqual([]);
    expect(review.messages()).toHaveLength(4);
  });

  it("refuses a Free plan at the command too, if a request is sent anyway", () => {
    const replay = replayPort();
    const messages = reviewedMessages();
    const countReply = replay.port("count", { plan: "free", messages });
    expect(countReply.ok).toBe(false);
    expect(countReply.ok === false && countReply.errorCode).toBe("pro_required");

    const token = (
      transcript.exchanges.find(
        (exchange) => exchange.command === "count" && exchange.request.plan === "pro",
      )!.reply as { ok: true; result: MarkedDeletionCount }
    ).result.confirmationToken;
    const deleteReply = replay.port("delete", {
      plan: "free",
      messages,
      confirmationToken: token,
      confirmed: true,
    });
    expect(deleteReply.ok).toBe(false);
    expect(deleteReply.ok === false && deleteReply.errorCode).toBe("pro_required");
  });

  it("shows Pro Delete marked messages, and pressing it counts instead of deleting", () => {
    const { review, replay } = screen("pro");

    expect(review.controls()).toEqual([{ id: DELETE_MARKED_CONTROL, label: DELETE_MARKED_LABEL }]);
    expect(review.markup()).toContain(DELETE_MARKED_LABEL);

    review.press(DELETE_MARKED_CONTROL);

    expect(replay.calls.map((call) => call.command)).toEqual(["count"]);
    expect(review.phase()).toBe("confirming");
    const count = review.finalCount()!;
    expect(count.markedCount).toBe(2);
    expect(count.keptCount).toBe(1);
    expect(count.pendingCount).toBe(1);
    expect(count.confirmationPrompt).toBe("Delete 2 marked messages? This cannot be undone.");
    expect(count.confirmationRequired).toBe(true);
    expect(review.messages()).toHaveLength(4);
  });

  it("offers Delete these messages and Cancel once the count is on screen", () => {
    const { review } = screen("pro");
    review.press(DELETE_MARKED_CONTROL);

    expect(review.controls()).toEqual([
      { id: CONFIRM_DELETE_CONTROL, label: CONFIRM_DELETE_LABEL },
      { id: CANCEL_DELETE_CONTROL, label: CANCEL_DELETE_LABEL },
    ]);
    const markup = review.markup();
    expect(markup).toContain(CONFIRM_DELETE_LABEL);
    expect(markup).toContain(CANCEL_DELETE_LABEL);
    expect(markup).toContain("Delete 2 marked messages? This cannot be undone.");
  });

  it("leaves every fixture message intact when Pro presses Cancel", () => {
    const { review, replay } = screen("pro");
    const before = review.messages();
    review.press(DELETE_MARKED_CONTROL);

    review.press(CANCEL_DELETE_CONTROL);

    expect(review.messages()).toEqual(before);
    expect(review.messages()).toHaveLength(4);
    expect(replay.calls.map((call) => call.command)).toEqual(["count"]);
    expect(review.messageLocators()).toEqual([
      "discord-account-alpha-1444 0",
      "discord-account-alpha-1444 1",
      "telegram-account-beta-1444 0",
      "telegram-account-beta-1444 1",
    ]);
    expect(review.deletionOutcome()).toBeNull();
    expect(review.phase()).toBe("idle");
    expect(review.statusLine()).toBe(CANCELLED_LINE);
    // Cancel drops the confirmation token with it, so confirming later means a
    // fresh count of whatever is marked then.
    expect(review.finalCount()).toBeNull();
    expect(review.controls()).toEqual([{ id: DELETE_MARKED_CONTROL, label: DELETE_MARKED_LABEL }]);
    expect(() => review.press(CONFIRM_DELETE_CONTROL)).toThrow(/not on this screen/);
    expect(review.messages()).toHaveLength(4);
  });

  it("deletes exactly the two marked messages when Pro confirms", () => {
    const { review, replay } = screen("pro");
    review.press(DELETE_MARKED_CONTROL);
    review.press(CANCEL_DELETE_CONTROL);
    review.press(DELETE_MARKED_CONTROL);

    review.press(CONFIRM_DELETE_CONTROL);

    expect(replay.calls.map((call) => call.command)).toEqual(["count", "count", "delete"]);
    const outcome = review.deletionOutcome()!;
    expect(outcome.deletedCount).toBe(2);
    expect(outcome.keptUntouchedCount).toBe(2);
    expect(review.messageLocators()).toEqual([
      "discord-account-alpha-1444 1",
      "telegram-account-beta-1444 1",
    ]);
    expect(review.phase()).toBe("deleted");
    expect(review.statusLine()).toBe("2 marked messages deleted.");
    expect(review.controls()).toEqual([]);
  });
});
