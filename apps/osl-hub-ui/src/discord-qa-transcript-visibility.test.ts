import { describe, expect, it } from "vitest";
import fs from "node:fs";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function region(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  expect(start).toBeGreaterThan(-1);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

const headerControls = region(
  "function nativeDiscordHeaderControls()",
  "function trustedHeader()",
);
const visibilityToggle = region(
  "async function toggleDiscordQaTranscriptVisibility()",
  "void listen<void>(NATIVE_DISCORD_OVERLAY_CLOSED_EVENT",
);
const visibilityRefresh = region(
  "async function refreshDiscordQaTranscriptVisibility(",
  "const discordQaGeometryKeeper = createDiscordQaGeometryKeeper(",
);
const overlayClosed = region(
  "void listen<void>(NATIVE_DISCORD_OVERLAY_CLOSED_EVENT",
  "async function toggleDiscordQaComposer()",
);
const composerToggle = region(
  "async function toggleDiscordQaComposer()",
  "async function openSoleVerifiedDiscordQaOverlay()",
);

type EyeMarkup = { transcriptNotice: string; transcriptVisibilityControl: string };

/**
 * Evaluate the shipped eye markup block itself, so "the control reflects the
 * active mode" is asserted against the real template rather than a copy of it.
 * Only the TypeScript annotation is stripped; the logic is untouched.
 */
function renderEye(input: {
  transcriptVisible: boolean;
  verifiedPeer: boolean;
  visibilityBusy?: boolean;
  outcome?: "applied" | "unapplied" | "failed";
}): EyeMarkup {
  const block = region(
    '  const transcriptMode = transcriptVisible ? "plaintext" : "flagtext";',
    "\n  const lock =",
  ).replace(": DiscordQaTranscriptVisibilityOutcome", "");
  const build = new Function(
    "transcriptVisible",
    "verifiedPeer",
    "visibilityBusy",
    "discordQaTranscriptVisibilityOutcome",
    "eye",
    `${block}\nreturn { transcriptNotice, transcriptVisibilityControl };`,
  ) as (
    transcriptVisible: boolean,
    verifiedPeer: unknown,
    visibilityBusy: boolean,
    outcome: string,
    eye: string,
  ) => EyeMarkup;
  return build(
    input.transcriptVisible,
    input.verifiedPeer ? { personId: "person-1" } : null,
    input.visibilityBusy ?? false,
    input.outcome ?? "applied",
    "<svg></svg>",
  );
}

describe("Discord QA transcript visibility (the green eye)", () => {
  it("renders the active transcript mode on the control itself", () => {
    const shown = renderEye({ transcriptVisible: true, verifiedPeer: true });
    const hidden = renderEye({ transcriptVisible: false, verifiedPeer: true });

    expect(shown.transcriptVisibilityControl).toContain('id="discord-qa-transcript-visibility"');
    expect(shown.transcriptVisibilityControl).toContain('data-transcript-mode="plaintext"');
    expect(shown.transcriptVisibilityControl).toContain('aria-pressed="true"');
    expect(shown.transcriptVisibilityControl).toContain("discord-qa-icon-control visible");
    expect(shown.transcriptVisibilityControl).toContain("decrypted text");

    expect(hidden.transcriptVisibilityControl).toContain('data-transcript-mode="flagtext"');
    expect(hidden.transcriptVisibilityControl).toContain('aria-pressed="false"');
    expect(hidden.transcriptVisibilityControl).toContain("discord-qa-icon-control hidden");
    expect(hidden.transcriptVisibilityControl).toContain("flagtext");

    // The slashed eye glyph is the mode's colour-independent signal.
    expect(headerControls).toContain(
      `\${transcriptVisible ? "" : '<path class="qa-icon-slash" d="M4 4l16 16"/>'}`,
    );
  });

  it("shows a failed toggle on the control instead of only in an occluded toast", () => {
    const failed = renderEye({ transcriptVisible: true, verifiedPeer: true, outcome: "failed" });

    expect(failed.transcriptVisibilityControl).toContain('data-transcript-state="failed"');
    expect(failed.transcriptVisibilityControl).toContain('aria-invalid="true"');
    expect(failed.transcriptVisibilityControl).toContain("failed closed");
    expect(failed.transcriptNotice).toContain('role="status"');
    expect(failed.transcriptNotice).toContain("Eye failed");

    // Every failure path marks the control, and none of them leaves the header
    // claiming success.
    expect(visibilityToggle).toContain('discordQaTranscriptVisibilityOutcome = "failed"');
    expect(visibilityToggle.match(/discordQaTranscriptVisibilityOutcome = "failed"/gu) ?? [])
      .toHaveLength(3);
    expect(visibilityToggle).toContain("Transcript visibility failed closed");
  });

  it("says so when the mode is saved but no display surface exists to render it", () => {
    const unapplied = renderEye({ transcriptVisible: false, verifiedPeer: true, outcome: "unapplied" });

    expect(unapplied.transcriptVisibilityControl).toContain('data-transcript-state="unapplied"');
    expect(unapplied.transcriptNotice).toContain("no display surface open");
    expect(unapplied.transcriptVisibilityControl).toContain("no protected display surface");
    // "unapplied" means the surface does not exist — never "the lock is off".
    expect(unapplied.transcriptVisibilityControl).not.toContain("lock");
    expect(unapplied.transcriptNotice).not.toContain("lock");
    expect(visibilityToggle).toContain("const transcriptSurfaceLive = nativeDiscordOverlaySurfacePresent");
    expect(visibilityToggle).toContain(
      'discordQaTranscriptVisibilityOutcome = transcriptSurfaceLive ? "applied" : "unapplied"',
    );
  });

  it("works with the lock off: encryption and display are independent", () => {
    // The eye reads the display surface's own presence and never the lock, so
    // every notify/outcome branch turns on the surface flag alone.
    expect(visibilityToggle).not.toContain("nativeDiscordProtectionActive");
    expect(visibilityToggle).toContain('=== "1"\n    && transcriptSurfaceLive');
    expect(visibilityToggle).toContain('VITE_OSL_DISCORD_QA_SHELL === "1" && transcriptSurfaceLive');
    expect(visibilityToggle).toContain(
      'if (transcriptSurfaceLive && import.meta.env.VITE_OSL_DISCORD_QA_SHELL !== "1")',
    );
    // The surface flag is its own lifecycle: opened with protection, cleared
    // only by a real overlay close or a context teardown — never by the lock.
    expect(source).toContain("let nativeDiscordOverlaySurfacePresent = false;");
    expect(source.match(/nativeDiscordOverlaySurfacePresent = true/gu) ?? []).toHaveLength(2);
    expect(overlayClosed).toContain("nativeDiscordOverlaySurfacePresent = false;");
    expect(region("function resetLocalProtectedSheet(", "async function closeActiveServiceSurface("))
      .toContain("nativeDiscordOverlaySurfacePresent = false;");
    // The QA lock toggle leaves the retained surface dormant, so it must not
    // clear the flag; the native side sends no overlay-closed event for it.
    expect(composerToggle).not.toContain("nativeDiscordOverlaySurfacePresent");
    expect(region("async function toggleLocalProtectedSheet()", "async function openNativeDiscordProtection("))
      .not.toContain("nativeDiscordOverlaySurfacePresent");
  });

  it("keeps the lock control out of the eye control's enabled state", () => {
    // The eye's disabled/enabled and its rendered mode depend on the verified
    // peer scope and its own in-flight write only.
    const eyeBlock = region(
      '  const transcriptMode = transcriptVisible ? "plaintext" : "flagtext";',
      "\n  const lock =",
    );
    expect(eyeBlock).not.toContain("nativeDiscordProtectionActive");
    expect(eyeBlock).not.toContain("discordQaComposer");
    expect(headerControls).toContain(
      '${!verifiedPeer || visibilityBusy ? "disabled" : ""}',
    );
    expect(headerControls).toContain("const transcriptVisible = peerProtectedSheet.decryptDisplayEnabled;");
  });

  it("stays disabled without a verified peer scope and never shows a stale outcome there", () => {
    const noPeer = renderEye({ transcriptVisible: true, verifiedPeer: false, outcome: "failed" });
    const busy = renderEye({ transcriptVisible: true, verifiedPeer: true, visibilityBusy: true });

    expect(noPeer.transcriptVisibilityControl).toContain("disabled");
    expect(noPeer.transcriptVisibilityControl).toContain('data-transcript-state="applied"');
    expect(noPeer.transcriptNotice).toBe("");
    expect(noPeer.transcriptVisibilityControl).toContain("verified friend");
    expect(busy.transcriptVisibilityControl).toContain("disabled");
  });

  it("drives the rendered transcript: notifies the transcript layer, persists, and repaints", () => {
    // State flip and repaint happen before the awaits, so the operator sees the
    // mode change immediately instead of after two IPC round trips.
    expect(visibilityToggle).toMatch(
      /peerProtectedSheet\.decryptDisplayEnabled = requested;\n\s*render\(\);/u,
    );
    expect(visibilityToggle).toContain('emitTo(\n      "native-discord-overlay",');
    expect(visibilityToggle).toContain("PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,\n      requested,");
    expect(visibilityToggle).toContain("await saveActiveContextSecurity(");
    // A rejected notify or save rolls the mode back and re-notifies the layer.
    expect(visibilityToggle).toContain("peerProtectedSheet.decryptDisplayEnabled = previous");
    expect(visibilityToggle).toContain("PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,\n        previous,");
    expect(visibilityToggle).toContain("|| !qaImmediate");
    // The stored policy, never the cache, decides what the header keeps.
    expect(visibilityToggle).toContain("peerProtectedSheet.ttlSeconds = saved.ttlSeconds");
    expect(visibilityToggle).toContain("peerProtectedSheet.decryptDisplayEnabled = saved.decryptDisplayEnabled");
  });

  it("never changes lock, protection, or composer state", () => {
    for (const forbidden of [
      // Not even read any more: the eye is independent of encryption.
      "nativeDiscordProtectionActive",
      "discordQaComposerBusy",
      "discordQaComposerOpening",
      "discordQaAutoComposer",
      "openDiscordQaComposer(",
      "toggleLocalProtectedSheet(",
      "resetLocalProtectedSheet(",
      "setNativeDiscordProtectedOverlayOpen(",
      "setNativeDiscordProtectedOverlayOpenForQa(",
      "discordQaOverlayState =",
      "activeContextToken =",
      "prepare",
      "console.",
    ]) {
      expect(visibilityToggle).not.toContain(forbidden);
      expect(visibilityRefresh).not.toContain(forbidden);
    }
  });

  it("keeps the lock from changing the eye's mode", () => {
    for (const body of [composerToggle, overlayClosed]) {
      expect(body).not.toContain("decryptDisplayEnabled");
      expect(body).not.toContain("PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT");
      // Nor may a lock move rewrite what the eye is reporting.
      expect(body).not.toContain("discordQaTranscriptVisibilityOutcome");
      expect(body).not.toContain("discordQaHeaderBusy");
    }
    // The lock's operator-vs-automatic flicker fix stays exactly as it was.
    expect(composerToggle).toContain("if (!discordQaShell || discordQaComposerBusy) return;");
    expect(source).toContain("const discordQaAutoComposerMaxAttempts = 3");
    expect(source).toContain('source: "operator" | "automatic" = "operator"');
    expect(source).toContain("if (!discordQaShell || discordQaComposerOpening || route !== \"service\") return;");
    // Closing the protected transcript layer in QA keeps the peer scope, so the
    // eye's mode survives a lock cycle.
    expect(source).toContain('if (discordQaShell) {\n          nativeDiscordProtectionActive = false;\n          discordQaOverlayState = "starting";');
  });

  it("reconciles the eye from the authoritative policy on the existing tick, with no new timer", () => {
    // get_native_discord_overlay_state rejects every caller that is not the
    // overlay window, so the main window must read the scope policy instead.
    expect(source).not.toContain("getNativeDiscordOverlayState");
    expect(visibilityRefresh).toContain("await loadActiveContextSecurity(active.context.contextToken)");
    expect(visibilityRefresh).toContain("isLocalTtlSeconds(security.ttlSeconds)");
    expect(visibilityRefresh).toContain("peerProtectedSheet.decryptDisplayEnabled = security.decryptDisplayEnabled");
    expect(visibilityRefresh).toContain("peerProtectedSheet.ttlSeconds = security.ttlSeconds");
    expect(visibilityRefresh).toContain("if (discordQaHeaderBusy) return");
    expect(visibilityRefresh).toContain("DISCORD_QA_SCOPE_SECURITY_MIN_INTERVAL_MS");
    expect(source).toContain("const DISCORD_QA_SCOPE_SECURITY_MIN_INTERVAL_MS = 5_000");
    // Rides the geometry keeper's single interval; adds none of its own. That
    // one cadence now lives inside the keeper itself, so the main window owns no
    // raw timer at all -- a timer this file cannot start is a timer it cannot
    // leak.
    expect(source).toContain("void refreshDiscordQaTranscriptVisibility();\n    return withNativeDeadline(");
    expect(source.match(/window\.setInterval\(/gu) ?? []).toHaveLength(0);
    expect(source).not.toContain("setTimeout(() => void refreshDiscordQaTranscriptVisibility");
  });

  it("leaves the composer-less lock-visibility gate inert", () => {
    expect(source).toContain("let discordMarkerAvailable = true;");
    expect(headerControls).toContain(
      "const composerControl = discordMarkerAvailable || nativeDiscordProtectionActive",
    );
    // The eye must not read or drive that gate.
    expect(visibilityToggle).not.toContain("discordMarkerAvailable");
    expect(visibilityRefresh).not.toContain("discordMarkerAvailable");
  });
});
