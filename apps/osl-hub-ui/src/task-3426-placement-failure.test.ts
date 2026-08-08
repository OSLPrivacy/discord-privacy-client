import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { blankLocalProtectedModel, localProtectedSheetMarkup } from "./local-protected-sheet";
import {
  PLACEMENT_NOT_SENT_SENTENCE,
  UNNAMED_APP,
  placeProtectedTextOrExplain,
  placementAppName,
  placementFailureNotice,
  placementFailureNoticeMarkup,
  type EditableBox,
  type PlacementFailureNotice,
} from "./placement-failure";

/**
 * TASK 3426 - what the person is told when OSL cannot place the text.
 *
 * The finish line is read off one screen: with placing forced to fail, the
 * screen names the app and says the text was not sent, the private box still
 * holds the person's own words, and the other app's box is empty.
 *
 * So these tests do not assert on a copy of the copy. They run the shipped
 * `placeProtectedTextOrExplain` with a placer forced to fail, then render the
 * shipped `localProtectedSheetMarkup` with whatever that produced, and read all
 * four items back out of the rendered screen.
 */

const styles = readFileSync(new URL("./local-protected-sheet.css", import.meta.url), "utf8");

/** The person's own words. Nothing in the notice may echo them back. */
const PRIVATE_TEXT = "MAPLE-3426 meet me at the north gate at nine";
const APP = "Discord";

/** A textarea double: the one property the module and the browser share. */
function box(value = ""): EditableBox {
  return { value };
}

/** Read one `<textarea id="...">…</textarea>` back out of rendered markup. */
function textareaContents(markup: string, id: string): string {
  const match = new RegExp(`<textarea id="${id}"[^>]*>([\\s\\S]*?)</textarea>`, "u").exec(markup);
  expect(match, `${id} should be on the screen`).not.toBeNull();
  return (match as RegExpExecArray)[1] as string;
}

function cssRule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
  const match = new RegExp(`(?:^|\\})\\s*${escaped}\\s*\\{([^}]*)\\}`, "mu").exec(styles);
  expect(match, `${selector} should have a rule in local-protected-sheet.css`).not.toBeNull();
  return (match as RegExpExecArray)[1] as string;
}

/**
 * One forced-failure run. `place` is the placer, forced to fail; `prefill` is
 * anything a half-finished write left in the other app's box before it gave up.
 */
async function forcedFailureRun(options: {
  place: () => never | Promise<never> | unknown;
  prefill?: string;
}): Promise<{
  notice: PlacementFailureNotice | null;
  shown: (PlacementFailureNotice | null)[];
  privateBox: EditableBox;
  otherAppBox: EditableBox;
  screen: string;
}> {
  const privateBox = box(PRIVATE_TEXT);
  const otherAppBox = box(options.prefill ?? "");
  const shown: (PlacementFailureNotice | null)[] = [];
  const notice = await placeProtectedTextOrExplain({
    appName: APP,
    privateText: PRIVATE_TEXT,
    privateBox,
    otherAppBox,
    place: options.place as never,
    show: (value) => { shown.push(value); },
  });
  const screen = localProtectedSheetMarkup({
    ...blankLocalProtectedModel(true),
    context: {
      contextToken: "local-token-3426",
      conversationId: "local-3426",
      serviceId: "discord",
      accountId: "discord-primary",
    } as never,
    chatLabel: "Rose",
    draft: privateBox.value,
    placementFailure: notice,
  });
  return { notice, shown, privateBox, otherAppBox, screen };
}

describe("TASK 3426 - the failed-placement message", () => {
  it("shows the message, keeps the private text, and leaves the other app's box empty", async () => {
    // Placing forced to fail: the placer refuses by name, the way the native
    // side does when no route is left.
    const run = await forcedFailureRun({
      place: () => ({
        placed: false as const,
        cause: "windowNotAvailable" as const,
        error: "Discord window #1 cannot be grabbed: front-window grab is only available on Windows",
      }),
      // A half-finished write already put part of the text in Discord's box.
      prefill: "MAPLE-3426 meet me at the n",
    });

    // 1. The screen shows the message, and it names the app.
    const noticeCount = [...run.screen.matchAll(/<section class="placement-failure"/gu)].length;
    expect(noticeCount).toBe(1);
    expect(run.screen).toContain(`data-placement-app="${APP}"`);
    expect(run.screen).toContain(`OSL could not put your message in ${APP}.`);
    const appMentions = [...run.screen.matchAll(new RegExp(APP, "gu"))].length;

    // 2. The screen says, in plain words, that the text was not sent.
    expect(run.screen).toContain(PLACEMENT_NOT_SENT_SENTENCE);
    expect(PLACEMENT_NOT_SENT_SENTENCE).toBe("Your message was not sent anywhere.");
    expect(run.screen).toContain(`Nothing was left in ${APP}.`);
    // role="alert" so it is announced, not just drawn.
    expect(run.screen).toMatch(/<section class="placement-failure" role="alert"/u);

    // 3. The private box still holds the person's text -- read off the screen,
    //    not off the model.
    expect(textareaContents(run.screen, "local-protected-draft")).toBe(PRIVATE_TEXT);
    expect(run.privateBox.value).toBe(PRIVATE_TEXT);

    // 4. The other app's box is empty, including the partial write.
    expect(run.otherAppBox.value).toBe("");
    expect(run.otherAppBox.value).toHaveLength(0);

    // Never fail quietly: the notice reached the sink as well as the return.
    expect(run.shown).toHaveLength(1);
    expect(run.shown[0]).toBe(run.notice);
    expect(run.notice).not.toBeNull();

    console.log(`task_3426_notice_count=${noticeCount}`);
    console.log(`task_3426_app_named=${(run.notice as PlacementFailureNotice).appName}`);
    console.log(`task_3426_app_mentions_on_screen=${appMentions}`);
    console.log(`task_3426_not_sent_sentence=${PLACEMENT_NOT_SENT_SENTENCE}`);
    console.log(`task_3426_screen_message=${(run.notice as PlacementFailureNotice).message}`);
    console.log(`task_3426_private_box_chars=${textareaContents(run.screen, "local-protected-draft").length}`);
    console.log(`task_3426_private_box_text=${textareaContents(run.screen, "local-protected-draft")}`);
    console.log(`task_3426_other_app_box_chars=${run.otherAppBox.value.length}`);
    console.log(`task_3426_silent_failures=${run.shown.filter((value) => value === null).length}`);
  });

  it("never fails quietly, whatever the placer returns", async () => {
    const returns: { label: string; place: () => unknown }[] = [
      { label: "null", place: () => null },
      { label: "undefined", place: () => undefined },
      { label: "throws", place: () => { throw new Error("SendInput was refused"); } },
      { label: "rejects", place: () => Promise.reject(new Error("the placer went away")) },
      { label: "placed-false", place: () => ({ placed: false }) },
      { label: "unreadable-shape", place: () => ({ ok: true }) },
      { label: "placed-truthy-not-true", place: () => ({ placed: "yes" }) },
    ];
    const quiet: string[] = [];
    for (const value of returns) {
      const run = await forcedFailureRun({ place: value.place });
      if (run.notice === null || run.shown.length !== 1 || run.shown[0] === null) quiet.push(value.label);
      expect(run.screen, value.label).toContain(PLACEMENT_NOT_SENT_SENTENCE);
      expect(run.screen, value.label).toContain(`OSL could not put your message in ${APP}.`);
      expect(textareaContents(run.screen, "local-protected-draft"), value.label).toBe(PRIVATE_TEXT);
      expect(run.otherAppBox.value, value.label).toBe("");
    }
    expect(quiet).toEqual([]);
    console.log(`task_3426_placer_answers_checked=${returns.length}`);
    console.log(`task_3426_quiet_failures=${quiet.length}`);
  });

  it("never leaves the private text in a box the person cannot see", async () => {
    const run = await forcedFailureRun({
      place: () => ({ placed: false as const, cause: "appRefused" as const, error: `Discord refused "${PRIVATE_TEXT}"` }),
      prefill: PRIVATE_TEXT,
    });
    // The other app's box holds nothing at all -- not the draft, not a fragment.
    expect(run.otherAppBox.value).toBe("");
    // And the notice does not reprint the draft, even though the refusal did.
    const notice = run.notice as PlacementFailureNotice;
    expect(notice.detail).not.toContain(PRIVATE_TEXT);
    expect(notice.detail).toContain("[redacted]");
    expect(notice.message).not.toContain(PRIVATE_TEXT);
    // The draft appears on screen exactly once: in the box the person is
    // looking at.
    const draftOnScreen = [...run.screen.matchAll(new RegExp(PRIVATE_TEXT.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&"), "gu"))].length;
    expect(draftOnScreen).toBe(1);
    console.log(`task_3426_private_text_in_other_app_box_chars=${run.otherAppBox.value.length}`);
    console.log(`task_3426_private_text_copies_on_screen=${draftOnScreen}`);
    console.log(`task_3426_redacted_detail=${notice.detail}`);
  });

  it("says nothing when placing worked", async () => {
    const privateBox = box(PRIVATE_TEXT);
    const otherAppBox = box("");
    const shown: (PlacementFailureNotice | null)[] = [];
    const notice = await placeProtectedTextOrExplain({
      appName: APP,
      privateText: PRIVATE_TEXT,
      privateBox,
      otherAppBox,
      place: () => {
        otherAppBox.value = PRIVATE_TEXT;
        return { placed: true as const };
      },
      show: (value) => { shown.push(value); },
    });
    expect(notice).toBeNull();
    expect(shown).toEqual([null]);
    expect(otherAppBox.value).toBe(PRIVATE_TEXT);
    const screen = localProtectedSheetMarkup({
      ...blankLocalProtectedModel(true),
      context: { contextToken: "local-token-3426" } as never,
      chatLabel: "Rose",
      draft: privateBox.value,
      placementFailure: notice,
    });
    expect(screen).not.toContain("placement-failure");
    expect(screen).not.toContain(PLACEMENT_NOT_SENT_SENTENCE);
  });

  it("names the app in every failure sentence, and never goes nameless", () => {
    for (const cause of ["windowNotAvailable", "messageBoxNotFound", "appRefused"] as const) {
      const notice = placementFailureNotice("Telegram", cause);
      expect(notice.headline).toContain("Telegram");
      expect(notice.reason).toContain("Telegram");
      expect(notice.notSent).toBe(PLACEMENT_NOT_SENT_SENTENCE);
      expect(notice.whereYourTextIs).toContain("Telegram");
    }
    // The placer being unavailable is not the app's doing, so its reason does
    // not blame the app -- but the headline still names where it was going.
    const unavailable = placementFailureNotice("WhatsApp", "placerUnavailable");
    expect(unavailable.headline).toContain("WhatsApp");
    expect(unavailable.message).toContain(PLACEMENT_NOT_SENT_SENTENCE);

    // A missing or unusable name still produces a sentence.
    expect(placementAppName("")).toBe(UNNAMED_APP);
    expect(placementAppName(undefined)).toBe(UNNAMED_APP);
    expect(placementFailureNotice(null).headline).toBe(`OSL could not put your message in ${UNNAMED_APP}.`);
  });

  it("escapes a hostile window title and bounds its length", () => {
    const hostile = `<img src=x onerror="alert(1)">`;
    const markup = placementFailureNoticeMarkup(placementFailureNotice(hostile, "appRefused"));
    expect(markup).not.toContain("<img");
    expect(markup).toContain("&lt;img");
    expect(placementAppName(`${"A".repeat(80)}`)).toHaveLength(65);
    expect(placementAppName("Discord\u0000\u202e")).toBe("Discord");
  });

  it("gives the notice a rule of its own, in danger colours", () => {
    expect(cssRule(".placement-failure")).toContain("--danger");
    expect(cssRule(".placement-failure-not-sent")).toContain("font-weight: 700");
  });
});
