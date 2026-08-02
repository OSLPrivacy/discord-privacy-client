import { describe, expect, it } from "vitest";
import fs from "node:fs";
import { inDomTooltipMarkup } from "./in-dom-tooltip";

/**
 * The hub half of `osl://native-discord-composer-unreachable`.
 *
 * The native side raises this when OSL's protected composer is on screen and is
 * NOT the window receiving keystrokes -- Windows refused it focus, or Discord is
 * drawing above it. Typing then goes to Discord in the clear; three plaintext
 * messages have already reached a real conversation that way.
 *
 * The payload is `{ reason, unreachable }` and was once a bare boolean. That is
 * the regression these tests exist to keep out: a handler that returned on any
 * non-boolean payload raised NOTHING on a real focus refusal, so the one warning
 * that says "your typing is going to Discord" was dark exactly when it mattered.
 * `unreachable` is the aggregate LEVEL across both native conditions, computed by
 * a single writer that emits only on the aggregate's edges, so the hub assigns it
 * rather than counting edges it cannot attribute.
 *
 * Same technique as the sibling header-strip tests: the shipped template and the
 * shipped handler are evaluated, never a copy of them.
 */
const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function region(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  expect(start).toBeGreaterThan(-1);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

/** The shipped chip template, with only its three inputs supplied. */
const noticeBody = region(
  '  if (!nativeDiscordProtectionActive || !nativeDiscordComposerUnreachable) return "";',
  "\n}\n\nfunction nativeDiscordHeaderControls()",
);

function renderNotice(protectionActive: boolean, unreachable: boolean, reason = ""): string {
  const build = new Function(
    "nativeDiscordProtectionActive",
    "nativeDiscordComposerUnreachable",
    "nativeDiscordComposerUnreachableReason",
    "inDomTooltipMarkup",
    noticeBody,
  ) as (protectionActive: boolean, unreachable: boolean, reason: string, tooltip: typeof inDomTooltipMarkup) => string;
  return build(protectionActive, unreachable, reason, inDomTooltipMarkup);
}

/**
 * The shipped listener body. Its one `as` cast is erased before evaluation --
 * exactly what the build does to it and the only TypeScript syntax in the region
 * -- and the erasure is asserted to have landed, so the harness cannot silently
 * stop matching the shipped source and start testing a stale copy.
 */
const CAST = " as { reason?: unknown; unreachable?: unknown }";
const listenerRegion = region(
  "    // `{ reason, unreachable }`, not the bare boolean this once was.",
  "\n    },\n  );",
);
expect(listenerRegion).toContain(CAST);
const listenerBody = listenerRegion.split(CAST).join("");

/**
 * Run the shipped listener body against a payload. Returns the arguments it
 * passed on to `applyNativeDiscordComposerUnreachable`, or `null` if it refused
 * the payload outright.
 */
function deliver(payload: unknown): [boolean, string] | null {
  let applied: [boolean, string] | null = null;
  const build = new Function(
    "payload",
    "NATIVE_DISCORD_COMPOSER_UNREACHABLE_REASONS",
    "applyNativeDiscordComposerUnreachable",
    listenerBody,
  ) as (
    payload: unknown,
    reasons: readonly string[],
    apply: (unreachable: boolean, reason: string) => void,
  ) => void;
  build(payload, ["zorder-band", "keyboard-focus", "session-ended"], (unreachable, reason) => {
    applied = [unreachable, reason];
  });
  return applied;
}

describe("native Discord composer-unreachable warning", () => {
  it("listens for the event the native side actually emits, on the window it emits to", () => {
    expect(source).toContain(
      'const NATIVE_DISCORD_COMPOSER_UNREACHABLE_EVENT = "osl://native-discord-composer-unreachable";',
    );
    // Emitted with `emit_to("main", ...)`, so main.ts is the window that must
    // listen, and the payload contract is `{ reason, unreachable }`.
    expect(source).toContain(
      "void listen<{ reason?: unknown; unreachable?: unknown }>(\n    NATIVE_DISCORD_COMPOSER_UNREACHABLE_EVENT,",
    );
    // The bare-boolean listener is what went dark when the shape changed: a
    // `typeof payload !== "boolean"` guard on this event can only refuse every
    // message the native side now sends.
    expect(source).not.toContain("void listen<boolean>(NATIVE_DISCORD_COMPOSER_UNREACHABLE_EVENT");
    // The reason set is closed, and matches the native `COMPOSER_UNREACHABLE_*`
    // constants exactly.
    expect(source).toContain(
      'const NATIVE_DISCORD_COMPOSER_UNREACHABLE_REASONS = ["zorder-band", "keyboard-focus", "session-ended"] as const;',
    );
  });

  it("reads the level out of the object payload instead of refusing it", () => {
    // THE REGRESSION. Both causes, exactly as the native side serialises them.
    expect(deliver({ reason: "keyboard-focus", unreachable: true })).toEqual([
      true,
      "keyboard-focus",
    ]);
    expect(deliver({ reason: "zorder-band", unreachable: true })).toEqual([true, "zorder-band"]);
    // Retractions land the same way, so a warning cannot outlive its condition.
    expect(deliver({ reason: "keyboard-focus", unreachable: false })).toEqual([
      false,
      "keyboard-focus",
    ]);
    expect(deliver({ reason: "session-ended", unreachable: false })).toEqual([
      false,
      "session-ended",
    ]);
  });

  it("takes the level as given, so a duplicated or dropped message is idempotent", () => {
    // The native writer reads the aggregate across both of its latches, emits on
    // that aggregate's edges only, and puts the aggregate itself on the wire.
    // There is nothing left for the hub to reconstruct -- which is all the count
    // this replaced ever did, and the count could bank a retraction it never saw
    // raised, or stick on.
    const apply = region(
      "function applyNativeDiscordComposerUnreachable(",
      "\n  void listen<{ reason?: unknown; unreachable?: unknown }>(",
    );
    expect(apply).toContain("nativeDiscordComposerUnreachable = unreachable;");
    expect(apply).not.toContain("Math.min");
    expect(apply).not.toContain("Math.max");
    expect(source).not.toContain("nativeDiscordComposerUnreachableConditions");
    expect(source).not.toContain("NATIVE_DISCORD_COMPOSER_UNREACHABLE_LATCHES");
  });

  it("ignores a payload that is not the level this contract promises", () => {
    // Inventing a raise or a retraction from an off-contract payload is worse than
    // ignoring it: one direction invents a leak warning, the other clears a real
    // one. The bare booleans below are the OLD contract; the native side no longer
    // sends them, and nothing may be inferred from one.
    expect(deliver(true)).toBeNull();
    expect(deliver(false)).toBeNull();
    expect(deliver(null)).toBeNull();
    expect(deliver(undefined)).toBeNull();
    expect(deliver("keyboard-focus")).toBeNull();
    expect(deliver({ reason: "keyboard-focus" })).toBeNull();
    expect(deliver({ unreachable: "true" })).toBeNull();
  });

  it("keeps the warning when only the reason is off-contract", () => {
    // The reason is wording; the level is the safety signal. A native side that
    // grew a fourth reason must sharpen nothing rather than warn about nothing.
    expect(deliver({ reason: "who-knows", unreachable: true })).toEqual([true, ""]);
    expect(deliver({ unreachable: true })).toEqual([true, ""]);
    expect(deliver({ reason: 7, unreachable: true })).toEqual([true, ""]);
  });

  it("commits the raise and the retraction synchronously rather than on the next frame", () => {
    const apply = region(
      "function applyNativeDiscordComposerUnreachable(",
      "\n  void listen<{ reason?: unknown; unreachable?: unknown }>(",
    );
    // Every deferred frame is a frame the operator may spend typing in the clear.
    expect(apply).toContain("renderNow();");
    expect(apply).not.toContain("render();");
    // Only the warned/not-warned transition repaints. The native side withholds
    // everything else, so this is the whole edge test now.
    expect(apply).toContain("if (previous === unreachable) return;");
    expect(apply).toContain("if (unreachable) {");
  });

  it("shows the chip only while a protected composer is actually on screen", () => {
    expect(renderNotice(false, false)).toBe("");
    expect(renderNotice(false, true)).toBe("");
    // Protection live but nothing raised: silent.
    expect(renderNotice(true, false)).toBe("");
    expect(renderNotice(true, true)).not.toBe("");
  });

  it("names the leak and the one thing that tells the two composers apart", () => {
    const notice = renderNotice(true, true, "keyboard-focus");
    // Plain language, and it says where the keystrokes are going.
    expect(notice).toContain("going to Discord, not OSL");
    // The cyan ring is the operator's only way to tell OSL's composer from
    // Discord's own, so the warning points at it instead of just alarming.
    expect(notice.toLowerCase()).toContain("cyan ring");
    // Unlike the chips beside it this one reports plaintext leaving the app right
    // now, so it is assertive and not merely a status line.
    expect(notice).toContain('role="alert"');
    // Shape as well as colour, and a state a QA probe can read without guessing.
    expect(notice).toContain('data-composer-input-state="unreachable"');
    expect(notice).toContain(">!<");
    // Fixed literal only: no draft, message, peer or conversation text.
    expect(noticeBody).not.toContain("escapeHtml");
    expect(noticeBody).not.toContain("peerProtectedSheet");
    expect(noticeBody).not.toContain("oslChatDraft");
  });

  it("names the actual cause in the tooltip now that the payload carries one", () => {
    // The visible line is identical in every case: it is the sentence the operator
    // has to act on, and it must not move or change length between causes. Only
    // the tooltip sharpens.
    const focus = renderNotice(true, true, "keyboard-focus");
    const zorder = renderNotice(true, true, "zorder-band");
    expect(focus).toContain("Windows refused OSL the keyboard");
    // Tooltip text is HTML-escaped because it is inserted into shipped markup.
    expect(zorder).toContain("Discord is drawing above OSL&#39;s composer");
    for (const notice of [focus, zorder, renderNotice(true, true)]) {
      expect(notice).toContain("Your typing is going to Discord, not OSL — check the cyan ring");
      expect(notice).toContain("goes to Discord unencrypted");
      expect(notice).toContain("click the composer with the cyan lock ring");
    }
    // No reason -- a level whose reason was absent or unrecognised -- keeps the
    // disjunction rather than guessing at one of the two.
    expect(renderNotice(true, true)).toContain(
      "Windows refused it focus, or Discord is drawing above it",
    );
    // Those three wordings are the only thing this template chooses, and each is a
    // fixed literal selected by a fixed reason.
    expect(noticeBody).toContain('=== "zorder-band"');
    expect(noticeBody).toContain('=== "keyboard-focus"');
  });

  it("renders in the shipping strip too, not only the QA one", () => {
    const headerControls = region(
      "function nativeDiscordHeaderControls(): string {",
      "function trustedHeader()",
    );
    // The shipping build borrows the same Discord window and is refused focus the
    // same way, so the warning belongs in both strips.
    expect(headerControls.match(/\$\{composerUnreachableNotice\}/gu)?.length).toBe(2);
    // Built before the build discriminator, so neither branch can miss it.
    const beforeBranch = headerControls.slice(0, headerControls.indexOf("if (!discordQaShell) {"));
    expect(beforeBranch).toContain("const composerUnreachableNotice = nativeDiscordComposerUnreachableNotice();");
  });
});
