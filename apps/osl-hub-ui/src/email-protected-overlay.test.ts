import { describe, expect, it } from "vitest";
import {
  EMAIL_OVERLAY_TTL_OPTIONS,
  EMAIL_SEND_MODES,
  applyEmailDraftEnter,
  emailDraftOverlayMarkup,
  emailProtectedOverlayMarkup,
  emailReadingOverlayMarkup,
  emailRecipientSummary,
  emailSendModeById,
  formatEmailOverlayCountdown,
  formatEmailOverlayTtl,
  initialEmailDraftOverlayState,
  initialEmailProtectedOverlayFixtureState,
  initialEmailReadingOverlayState,
  openEmailSendReview,
} from "./email-protected-overlay";

describe("TASK 1228 shared email overlay -- send modes (gate 1223)", () => {
  it("carries the same five modes, ids and names as EmailSendMode::ALL", () => {
    expect(EMAIL_SEND_MODES.map((mode) => mode.id)).toEqual([
      "manual",
      "double_enter",
      "experimental_single_enter",
      "instant",
      "match_typing",
    ]);
    expect(EMAIL_SEND_MODES.map((mode) => mode.name)).toEqual([
      "Manual",
      "Double Enter",
      "Experimental Single Enter",
      "Instant",
      "Match typing",
    ]);
  });

  it("rejects an unknown send mode id", () => {
    // @ts-expect-error -- deliberately invalid id
    expect(() => emailSendModeById("turbo")).toThrow(/unknown email send mode/);
  });

  it("Enter always inserts a line and never opens send review, in every mode", () => {
    for (const mode of EMAIL_SEND_MODES) {
      const state = { ...initialEmailDraftOverlayState(), sendMode: mode.id, body: "hello" };
      const next = applyEmailDraftEnter(state);
      expect(next.body).toBe("hello\n");
      expect(next.reviewOpen).toBe(false);
    }
  });

  it("only the named Send control opens send review", () => {
    const opened = openEmailSendReview(initialEmailDraftOverlayState());
    expect(opened.reviewOpen).toBe(true);
  });
});

describe("TASK 1228 shared email overlay -- recipient summary (gate 1220)", () => {
  it("names every visible recipient and only counts hidden ones", () => {
    const summary = emailRecipientSummary(["to@example.osl"], ["cc@example.osl"], ["bcc@example.osl"]);
    expect(summary.visibleRecipients).toEqual(["to@example.osl", "cc@example.osl"]);
    expect(summary.hiddenCount).toBe(1);
    expect(summary.distinctRecipientCount).toBe(3);
    expect(summary.visibleRecipients).not.toContain("bcc@example.osl");
  });

  it("deduplicates a recipient repeated across To/Cc/Bcc", () => {
    const summary = emailRecipientSummary(["same@example.osl"], ["same@example.osl"], ["same@example.osl"]);
    expect(summary.visibleRecipients).toEqual(["same@example.osl"]);
    expect(summary.distinctRecipientCount).toBe(1);
  });

  it("matches TASK 1220's own fixture: 2 visible, 1 hidden, 3 distinct", () => {
    const summary = emailRecipientSummary(
      ["to-task1220@oslprivacy.com"],
      ["cc-task1220@oslprivacy.com"],
      ["bcc-task1220@oslprivacy.com"],
    );
    expect(summary.visibleRecipients).toEqual(["to-task1220@oslprivacy.com", "cc-task1220@oslprivacy.com"]);
    expect(summary.hiddenCount).toBe(1);
    expect(summary.distinctRecipientCount).toBe(3);
  });
});

describe("TASK 1228 shared email overlay -- timers", () => {
  it("names every TTL option", () => {
    expect(EMAIL_OVERLAY_TTL_OPTIONS.map(formatEmailOverlayTtl)).toEqual(["5 minutes", "1 hour", "1 day", "7 days"]);
  });

  it("formats a countdown without a fractional second and never below zero", () => {
    expect(formatEmailOverlayCountdown(3_661)).toBe("1h 1m 1s");
    expect(formatEmailOverlayCountdown(59)).toBe("59s");
    expect(formatEmailOverlayCountdown(-5)).toBe("0s");
  });
});

describe("TASK 1228 shared email overlay -- markup", () => {
  it("draft overlay markup names itself 'draft overlay' and shows the send review summary", () => {
    const state = {
      ...initialEmailDraftOverlayState(),
      to: ["to@example.osl"],
      cc: [],
      bcc: ["bcc@example.osl"],
      body: "hi there",
      reviewOpen: true,
    };
    const markup = emailDraftOverlayMarkup(state);
    expect(markup).toContain('aria-label="draft overlay"');
    expect(markup).toContain(">draft overlay<");
    expect(markup).toContain("1 visible recipient(s), 1 hidden, 2 total.");
    expect(markup).toContain('data-open="true"');
    expect(markup).not.toContain("bcc@example.osl");
  });

  it("reading overlay markup names itself 'reading overlay' and shows the countdown and body", () => {
    const state = { ...initialEmailReadingOverlayState(), remainingSeconds: 125, body: "secret text" };
    const markup = emailReadingOverlayMarkup(state);
    expect(markup).toContain('aria-label="reading overlay"');
    expect(markup).toContain(">reading overlay<");
    expect(markup).toContain("Deletes in 2m 5s");
    expect(markup).toContain("secret text");
  });

  it("the combined fixture markup contains both overlays exactly once", () => {
    const markup = emailProtectedOverlayMarkup(initialEmailProtectedOverlayFixtureState());
    expect(markup.match(/aria-label="draft overlay"/g)).toHaveLength(1);
    expect(markup.match(/aria-label="reading overlay"/g)).toHaveLength(1);
  });

  it("an attached file is listed by name and size in both overlays", () => {
    const draftState = { ...initialEmailDraftOverlayState(), attachments: [{ name: "notes.pdf", sizeLabel: "212 KB" }] };
    const readingState = { ...initialEmailReadingOverlayState(), attachments: [{ name: "photo.png", sizeLabel: "1.2 MB" }] };
    expect(emailDraftOverlayMarkup(draftState)).toContain("notes.pdf");
    expect(emailDraftOverlayMarkup(draftState)).toContain("212 KB");
    expect(emailReadingOverlayMarkup(readingState)).toContain("photo.png");
    expect(emailReadingOverlayMarkup(readingState)).toContain("1.2 MB");
  });

  it("escapes recipient, subject and body text so markup cannot be injected", () => {
    const state = {
      ...initialEmailDraftOverlayState(),
      to: ["<script>@example.osl"],
      subject: '"><img>',
      body: "<b>bold</b>",
    };
    const markup = emailDraftOverlayMarkup(state);
    expect(markup).not.toContain("<script>@example.osl");
    expect(markup).not.toContain('"><img>');
    expect(markup).not.toContain("<b>bold</b>");
  });
});
