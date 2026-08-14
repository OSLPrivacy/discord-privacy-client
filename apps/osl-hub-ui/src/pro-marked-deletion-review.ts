/**
 * The Pro-only deletion step at the end of a results review (TASK 1450).
 *
 * The review session (TASK 1446) decides one result at a time: Keep, Mark for
 * deletion, Review next, Finish review. Nothing is deleted there. This module
 * is what happens after it: the marked results are handed to the Pro marked
 * deletion command (TASK 1449) as `count` and then, only after that count has
 * been shown, `delete`.
 *
 * Three rules this screen exists to hold:
 *
 * 1. On Free there is no deletion control at all. Not a greyed-out button, not
 *    a button that explains itself when pressed -- `controls()` is empty and
 *    the markup carries no `<button>`. A control that is on screen but refuses
 *    still teaches people to press it; one that was never drawn cannot be
 *    pressed by accident. The backend refuses Free as well (`pro_required`),
 *    so this is the outer of two gates, not the only one.
 * 2. Delete marked messages never deletes. It asks the command for the final
 *    count and shows it. Only then do Delete these messages and Cancel exist.
 * 3. Cancel deletes nothing and sends nothing. It drops the confirmation token
 *    with it, so getting back to the confirmation means asking for a fresh
 *    count -- the same "the count you confirmed is the set that is deleted"
 *    binding TASK 1449 enforces on its side.
 *
 * The message locators are carried through opaquely and never parsed or opened
 * (TASK 1445 stripped jump links from them upstream).
 */

import {
  ResultsReviewSession,
  flattenReviewResultStoreOutput,
  type ReviewResultItem,
  type ReviewResultStoreOutput,
} from "./results-review";

/** Matches `RequesterPlan` in apps/osl-hub/src/pro_marked_deletion.rs. */
export type RequesterPlan = "free" | "pro";

/** Matches `ReviewDecision` in apps/osl-hub/src/pro_marked_deletion.rs. */
export type ReviewedMessageDecision = "pending" | "kept" | "markedForDeletion";

/** One reviewed result, in the exact shape the command deserializes. */
export interface ReviewedMessage {
  readonly accountId: string;
  readonly messageLocator: string;
  readonly decision: ReviewedMessageDecision;
  readonly reviewed: boolean;
}

export interface MarkedMessageRef {
  readonly accountId: string;
  readonly messageLocator: string;
}

export interface MarkedAccountCount {
  readonly accountId: string;
  readonly markedCount: number;
}

/** The `count` reply payload. */
export interface MarkedDeletionCount {
  readonly markedCount: number;
  readonly keptCount: number;
  readonly pendingCount: number;
  readonly accountCounts: readonly MarkedAccountCount[];
  readonly messages: readonly MarkedMessageRef[];
  readonly confirmationToken: string;
  readonly confirmationPrompt: string;
  readonly confirmationRequired: boolean;
}

/** The `delete` reply payload. */
export interface MarkedDeletionOutcome {
  readonly deletedCount: number;
  readonly deleted: readonly MarkedMessageRef[];
  readonly keptUntouchedCount: number;
  readonly confirmationToken: string;
}

export interface MarkedDeletionCountRequest {
  readonly plan: RequesterPlan;
  readonly messages: readonly ReviewedMessage[];
}

export interface MarkedDeletionDeleteRequest extends MarkedDeletionCountRequest {
  readonly confirmationToken: string;
  readonly confirmed: boolean;
}

export type MarkedDeletionReply =
  | { readonly ok: true; readonly command: string; readonly result: unknown }
  | { readonly ok: false; readonly command: string; readonly errorCode: string; readonly error: string };

/**
 * How this screen reaches `run_pro_marked_deletion_command`. One function, so
 * the transport (tauri invoke in the app, the recorded backend replies in the
 * check) is the only thing that changes.
 */
export type MarkedDeletionCommandPort = (
  command: "count" | "delete",
  request: MarkedDeletionCountRequest | MarkedDeletionDeleteRequest,
) => MarkedDeletionReply;

export const DELETE_MARKED_CONTROL = "delete-marked-messages";
export const CONFIRM_DELETE_CONTROL = "delete-these-messages";
export const CANCEL_DELETE_CONTROL = "cancel-deletion";

export const DELETE_MARKED_LABEL = "Delete marked messages";
export const CONFIRM_DELETE_LABEL = "Delete these messages";
export const CANCEL_DELETE_LABEL = "Cancel";

export const FREE_NO_DELETION_LINE =
  "Deleting marked messages is part of Pro. On Free you can review and mark, and nothing is deleted.";
export const CANCELLED_LINE = "Cancelled. Nothing was deleted.";

export interface DeletionControl {
  readonly id: string;
  readonly label: string;
}

export type DeletionPhase = "idle" | "confirming" | "deleted";

export interface ProMarkedDeletionReviewOptions {
  readonly plan: RequesterPlan;
  readonly messages: readonly ReviewedMessage[];
  readonly port: MarkedDeletionCommandPort;
}

/**
 * Drives a TASK 1446 review session and remembers what it decided, so the
 * decisions can be handed to the deletion command. The session itself only
 * reports per-account totals; deletion needs the individual results, and the
 * screen is the only place that knows which result was open when a decision
 * was made.
 */
export class ReviewDecisionRecorder {
  private readonly session: ResultsReviewSession;
  private readonly order: readonly ReviewResultItem[];
  private readonly locators = new Map<string, string>();
  private readonly decisions = new Map<string, ReviewedMessageDecision>();

  constructor(output: ReviewResultStoreOutput) {
    this.session = new ResultsReviewSession(output);
    // The review's own ids, in the review's own order, straight from the
    // session's flattener -- rebuilding them here would fork the id format.
    this.order = flattenReviewResultStoreOutput(output);
    const seenPerAccount = new Map<string, number>();
    for (const item of this.order) {
      const index = seenPerAccount.get(item.accountId) ?? 0;
      seenPerAccount.set(item.accountId, index + 1);
      // The id separator is a NUL byte, which has no business travelling
      // through a JSON command, so the locator is the same account-and-place
      // pair written plainly. Still opaque: nothing parses or opens it.
      this.locators.set(item.id, `${item.accountId} ${index}`);
    }
  }

  currentItem(): ReviewResultItem | null {
    return this.session.currentItem();
  }

  keep(): void {
    this.record("kept");
    this.session.keep();
  }

  markForDeletion(): void {
    this.record("markedForDeletion");
    this.session.markForDeletion();
  }

  reviewNext(): ReviewResultItem | null {
    return this.session.reviewNext();
  }

  finishReview(): void {
    this.session.finishReview();
  }

  private record(decision: ReviewedMessageDecision): void {
    const item = this.session.currentItem();
    if (!item) throw new Error("no result is open to decide");
    this.decisions.set(item.id, decision);
  }

  /**
   * Every result the review saw, decided or not, in review order. The locator
   * is the review's own opaque result id -- never a link, never a path.
   */
  reviewedMessages(): ReviewedMessage[] {
    return this.order.map((item) => {
      const decision = this.decisions.get(item.id) ?? "pending";
      return {
        accountId: item.accountId,
        messageLocator: this.locators.get(item.id) ?? item.id,
        decision,
        reviewed: decision !== "pending",
      };
    });
  }
}

function isRefusal(
  reply: MarkedDeletionReply,
): reply is { ok: false; command: string; errorCode: string; error: string } {
  return reply.ok === false;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

/** The deletion step of the review screen. */
export class ProMarkedDeletionReview {
  private readonly requesterPlan: RequesterPlan;
  private readonly port: MarkedDeletionCommandPort;
  private readonly reviewed: readonly ReviewedMessage[];
  private remaining: ReviewedMessage[];
  private currentPhase: DeletionPhase = "idle";
  private count: MarkedDeletionCount | null = null;
  private outcome: MarkedDeletionOutcome | null = null;
  private status: string | null = null;

  constructor(options: ProMarkedDeletionReviewOptions) {
    this.requesterPlan = options.plan;
    this.port = options.port;
    this.reviewed = options.messages.map((message) => ({ ...message }));
    this.remaining = this.reviewed.map((message) => ({ ...message }));
  }

  plan(): RequesterPlan {
    return this.requesterPlan;
  }

  phase(): DeletionPhase {
    return this.currentPhase;
  }

  /** The messages this screen still has. Deletion is the only thing that shrinks it. */
  messages(): ReviewedMessage[] {
    return this.remaining.map((message) => ({ ...message }));
  }

  messageLocators(): string[] {
    return this.remaining.map((message) => message.messageLocator);
  }

  finalCount(): MarkedDeletionCount | null {
    return this.count;
  }

  deletionOutcome(): MarkedDeletionOutcome | null {
    return this.outcome;
  }

  /**
   * The buttons on screen right now. Empty on Free: the deletion control is
   * not drawn at all, in any phase.
   */
  controls(): DeletionControl[] {
    if (this.requesterPlan !== "pro") return [];
    if (this.currentPhase === "confirming") {
      return [
        { id: CONFIRM_DELETE_CONTROL, label: CONFIRM_DELETE_LABEL },
        { id: CANCEL_DELETE_CONTROL, label: CANCEL_DELETE_LABEL },
      ];
    }
    if (this.currentPhase === "deleted") return [];
    return [{ id: DELETE_MARKED_CONTROL, label: DELETE_MARKED_LABEL }];
  }

  hasControl(id: string): boolean {
    return this.controls().some((control) => control.id === id);
  }

  statusLine(): string {
    if (this.requesterPlan !== "pro") return FREE_NO_DELETION_LINE;
    if (this.status) return this.status;
    if (this.currentPhase === "confirming" && this.count) return this.count.confirmationPrompt;
    return "Review finished. Nothing has been deleted yet.";
  }

  /** Only a control that is actually on screen can be pressed. */
  press(id: string): void {
    if (!this.hasControl(id)) {
      throw new Error(`${id} is not on this screen`);
    }
    if (id === DELETE_MARKED_CONTROL) {
      this.askForCount();
      return;
    }
    if (id === CANCEL_DELETE_CONTROL) {
      this.cancel();
      return;
    }
    this.confirmDeletion();
  }

  /** Step 1: ask the command for the final count and show it. Deletes nothing. */
  private askForCount(): void {
    const request: MarkedDeletionCountRequest = {
      plan: this.requesterPlan,
      messages: this.remaining.map((message) => ({ ...message })),
    };
    const reply = this.port("count", request);
    if (isRefusal(reply)) {
      this.count = null;
      this.currentPhase = "idle";
      this.status = reply.error;
      return;
    }
    this.count = reply.result as MarkedDeletionCount;
    this.currentPhase = "confirming";
    this.status = null;
  }

  /**
   * Cancel. No command call at all, and the token goes with it: confirming
   * again means a fresh count, so what is confirmed is always what was counted.
   */
  private cancel(): void {
    this.count = null;
    this.currentPhase = "idle";
    this.status = CANCELLED_LINE;
  }

  /** Step 2: the only path that deletes, and only with the token step 1 issued. */
  private confirmDeletion(): void {
    const count = this.count;
    if (!count) throw new Error("no final count has been shown");
    const request: MarkedDeletionDeleteRequest = {
      plan: this.requesterPlan,
      messages: this.remaining.map((message) => ({ ...message })),
      confirmationToken: count.confirmationToken,
      confirmed: true,
    };
    const reply = this.port("delete", request);
    if (isRefusal(reply)) {
      this.count = null;
      this.currentPhase = "idle";
      this.status = reply.error;
      return;
    }
    const outcome = reply.result as MarkedDeletionOutcome;
    const deleted = new Set(outcome.deleted.map((item) => `${item.accountId} ${item.messageLocator}`));
    this.remaining = this.remaining.filter(
      (message) => !deleted.has(`${message.accountId} ${message.messageLocator}`),
    );
    this.outcome = outcome;
    this.count = null;
    this.currentPhase = "deleted";
    this.status =
      outcome.deletedCount === 1
        ? "1 marked message deleted."
        : `${outcome.deletedCount} marked messages deleted.`;
  }

  markup(): string {
    return proMarkedDeletionReviewMarkup(this);
  }
}

function controlsMarkup(review: ProMarkedDeletionReview): string {
  const controls = review.controls();
  if (!controls.length) return "";
  const buttons = controls
    .map(
      (control) =>
        `<button class="button pmd-control" type="button" data-marked-deletion-action="${escapeHtml(control.id)}">${escapeHtml(control.label)}</button>`,
    )
    .join("");
  return `<div class="pmd-actions">${buttons}</div>`;
}

function countMarkup(review: ProMarkedDeletionReview): string {
  const count = review.finalCount();
  if (!count || review.phase() !== "confirming") return "";
  const perAccount = count.accountCounts
    .map(
      (entry) =>
        `<li><strong>${escapeHtml(entry.accountId)}</strong><span>${entry.markedCount} marked</span></li>`,
    )
    .join("");
  return `<p class="pmd-prompt">${escapeHtml(count.confirmationPrompt)}</p><ul class="pmd-account-counts">${perAccount}</ul><p class="pmd-kept">${count.keptCount} kept and ${count.pendingCount} not reviewed are left alone.</p>`;
}

export function proMarkedDeletionReviewMarkup(review: ProMarkedDeletionReview): string {
  return `<section class="pmd-review" aria-labelledby="pmd-title"><h2 id="pmd-title">Marked for deletion</h2>${countMarkup(review)}${controlsMarkup(review)}<p class="pmd-status" role="status">${escapeHtml(review.statusLine())}</p></section>`;
}
