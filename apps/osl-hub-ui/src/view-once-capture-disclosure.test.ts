/**
 * TASK 6844 - the viewer and the sender must see what screenshot detection can
 * and cannot do, before they use view-once, in the same words the Rust side
 * signs and checks.
 *
 * The Rust source is read directly rather than duplicated here. A test that
 * compares two hand-written copies of a sentence passes forever while the
 * shipped sentence rots.
 */
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import { oslChatsViewMarkup } from "./osl-chats-view";
import {
  CAPTURE_DISCLOSURE_SENDER,
  CAPTURE_DISCLOSURE_VIEWER,
  UNSUPPORTED_CAPTURE_PATHS,
} from "./view-once-capture-disclosure";
import { EMPTY_VIEW_ONCE_OVERLAY, viewOnceOverlayMarkup } from "./view-once-overlay";

const rustDisclosure = readFileSync(
  new URL("../../../crates/view-once-capture/src/disclosure.rs", import.meta.url),
  "utf8",
);

/**
 * Rust writes these as `"\` line-continuation strings, so the shipped value is
 * the concatenation of the lines with the leading backslash-newline removed.
 */
function rustConst(name: string): string {
  const start = rustDisclosure.indexOf(`pub const ${name}: &str = "\\\n`);
  expect(start, `missing ${name} in disclosure.rs`).toBeGreaterThan(-1);
  const bodyStart = rustDisclosure.indexOf('"\\\n', start) + 3;
  const bodyEnd = rustDisclosure.indexOf('";', bodyStart);
  expect(bodyEnd, `unterminated ${name}`).toBeGreaterThan(bodyStart);
  return rustDisclosure
    .slice(bodyStart, bodyEnd)
    .split("\\\n")
    .join("")
    .split("\n")
    .join("");
}

function rustList(name: string): string[] {
  const start = rustDisclosure.indexOf(name);
  expect(start, `missing ${name}`).toBeGreaterThan(-1);
  const open = rustDisclosure.indexOf("[", start);
  const close = rustDisclosure.indexOf("];", open);
  return [...rustDisclosure.slice(open, close).matchAll(/"([^"]*)"/gu)].map((match) => match[1]!);
}

/**
 * The one hand-written list of over-promising phrases lives in `disclosure.rs`.
 * Reading it here means this file cannot drift from the Rust check, and keeps
 * the phrases out of the app source the claim gate scans.
 */
const ABSOLUTE_CAPTURE_CLAIMS = rustList("ABSOLUTE_CAPTURE_CLAIMS");

function absoluteCaptureClaimsIn(text: string): string[] {
  const haystack = text.toLowerCase();
  return ABSOLUTE_CAPTURE_CLAIMS.filter((claim) => haystack.includes(claim));
}

function disclosesCaptureLimits(text: string): boolean {
  const haystack = text.toLowerCase();
  return (
    haystack.includes("only the screen-capture paths windows reports")
    && UNSUPPORTED_CAPTURE_PATHS.every((path) => haystack.includes(path))
    && haystack.includes("does not stop screenshots")
    && absoluteCaptureClaimsIn(text).length === 0
  );
}

describe("TASK 6844 the capture-detection disclosure the app ships", () => {
  it("matches the Rust copy the check verifies, byte for byte", () => {
    expect(CAPTURE_DISCLOSURE_VIEWER).toBe(rustConst("CAPTURE_DISCLOSURE_VIEWER"));
    expect(CAPTURE_DISCLOSURE_SENDER).toBe(rustConst("CAPTURE_DISCLOSURE_SENDER"));
  });

  it("bans a real list of phrases, taken from the Rust check", () => {
    expect(ABSOLUTE_CAPTURE_CLAIMS.length).toBeGreaterThanOrEqual(20);
    expect([...UNSUPPORTED_CAPTURE_PATHS]).toEqual(rustList("UNSUPPORTED_CAPTURE_PATHS"));
  });

  it("names every capture path OSL cannot see and never claims prevention", () => {
    for (const text of [CAPTURE_DISCLOSURE_VIEWER, CAPTURE_DISCLOSURE_SENDER]) {
      expect(disclosesCaptureLimits(text)).toBe(true);
      expect(absoluteCaptureClaimsIn(text)).toEqual([]);
      for (const path of UNSUPPORTED_CAPTURE_PATHS) expect(text.toLowerCase()).toContain(path);
      expect(text).toContain("does not stop screenshots");
    }
    expect(CAPTURE_DISCLOSURE_SENDER).toContain("No notification does not mean no copy was made.");
  });

  it("catches an absolute claim wherever it is written", () => {
    expect(absoluteCaptureClaimsIn("OSL detects all screenshots.")).toEqual(["detects all screenshots"]);
    expect(absoluteCaptureClaimsIn("This is Screenshot-Proof.")).toEqual(["screenshot-proof"]);
    expect(disclosesCaptureLimits(`${CAPTURE_DISCLOSURE_VIEWER} You will always know.`)).toBe(false);
    expect(disclosesCaptureLimits(CAPTURE_DISCLOSURE_VIEWER.replace(", and OSL does not stop screenshots", "")))
      .toBe(false);
  });
});

describe("TASK 6844 the viewer sees the limitation before the content", () => {
  const markup = viewOnceOverlayMarkup(
    { ...EMPTY_VIEW_ONCE_OVERLAY, open: true, content: { kind: "text", text: "Secret note" } },
    {
      duration_label: "View duration",
      duration_option_seconds_template: "{seconds} seconds",
      play_aria_label: "Play view once",
      close_aria_label: "Close view once overlay",
      protected_text_placeholder: "Protected text",
      protected_image_alt: "Protected image",
      empty_copy: "Nothing to show",
    },
  );

  it("renders the disclosure in the open overlay", () => {
    expect(markup).toContain("data-voo-capture-disclosure");
    expect(markup).toContain("OSL does not stop screenshots");
    expect(markup).toContain("cannot detect a camera pointed at your screen");
  });

  it("puts it above the control that reveals the content", () => {
    expect(markup.indexOf("data-voo-capture-disclosure")).toBeLessThan(markup.indexOf("data-voo-play"));
  });

  it("makes no absolute claim anywhere in the overlay", () => {
    expect(absoluteCaptureClaimsIn(markup)).toEqual([]);
  });
});

describe("TASK 6844 the sender sees the limitation before sending", () => {
  const markup = oslChatsViewMarkup({
    friends: [{
      personId: "friend-1",
      nickname: "Rose",
      verified: true,
      ready: true,
      preview: "See you soon",
      previewVisible: true,
      unreadCount: 0,
    }],
    activePersonId: "friend-1",
    messages: [],
    draft: "hello",
    busy: false,
    viewOnce: false,
  });

  it("renders the disclosure inside the view-once control", () => {
    expect(markup).toContain("data-osl-chat-capture-disclosure");
    expect(markup).toContain("No notification does not mean no copy was made.");
  });

  it("puts it before the send button", () => {
    expect(markup.indexOf("data-osl-chat-capture-disclosure"))
      .toBeLessThan(markup.indexOf("osl-chat-send"));
  });

  it("makes no absolute claim anywhere in the composer", () => {
    expect(absoluteCaptureClaimsIn(markup)).toEqual([]);
  });
});
