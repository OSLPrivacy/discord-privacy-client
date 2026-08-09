// The OSL Strip's behaviour contract (canon README "Screens / Views > 4.
// Strip", "Behaviors that must survive reimplementation"):
//
//   1. TOGGLE-TO-REVEAL — click reveals, a second click restores, and the
//      persisted state fails closed if the control becomes unavailable.
//   2. ROOM HONESTY — an unproven room greys timer/once/eye/whitelist/burn,
//      every greyed control keeps a tooltip that says why, and the composer
//      placeholder warns. Greyed means "OSL can't", never "you can't".
//   3. FACTS ARE MONOSPACE — chip faces render in the Consolas status style.
//   4. TOKENS ONLY — strip.css introduces no colour osl-tokens.ts does not
//      bless (the fixture's synthetic carrier chrome is deliberately excluded:
//      it uses the carrier's own colours and lives in the fixture page).
//
// There is deliberately no jsdom in this package, so the derivation logic is
// tested directly (strip-state.ts is pure) and the DOM interaction contract is
// asserted against strip.ts source the same way overlay.test.ts audits
// overlay.ts. The LIVE proof of toggle-to-reveal — real clicks in a real
// Chrome, text swapping and restoring — runs in
// screenshots/capture-strip.mjs, which fails its capture if the swap or the
// restore does not actually happen.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  deriveStripView,
  NO_ROOM_REASON,
  stripTimerFace,
  stripTimerWords,
  UNPROVEN_COMPOSER_PLACEHOLDER,
  type OslStripState,
} from "./strip-state";
import { allColours } from "./osl-tokens";

function readRelative(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

function baseState(overrides: Partial<OslStripState> = {}): OslStripState {
  return {
    roomProven: true,
    roomLabel: "#general",
    plan: "free",
    planAction: { available: true },
    homeAction: { available: true },
    burn: { available: true },
    whitelist: {
      available: true,
      roster: [
        { id: "mara", name: "Mara", build: "verified", buildLabel: "VERIFIED BUILD 0.9.4", allowed: true, matched: true },
        { id: "theo", name: "Theo", build: "verified", buildLabel: "VERIFIED BUILD 0.9.4", allowed: false, matched: true },
        { id: "jules", name: "Jules", build: "modified", buildLabel: "MODIFIED BUILD", allowed: false, matched: false },
      ],
    },
    timer: {
      available: true,
      seconds: 86_400,
      presets: [
        { label: "1H", seconds: 3_600, available: true },
        { label: "1D", seconds: 86_400, available: true },
      ],
    },
    once: { available: true, armed: false, seconds: 30 },
    lock: { state: "on", toggle: { available: true } },
    reveal: { available: true, revealed: false },
    quickSettings: [],
    windowControls: true,
    ...overrides,
  };
}

describe("room honesty", () => {
  it("greys timer, once, eye, whitelist and burn with the reason on every one", () => {
    const view = deriveStripView(baseState({ roomProven: false }));
    for (const chip of [view.timer, view.once, view.eye, view.whitelist, view.burn]) {
      expect(chip.disabled).toBe(true);
      expect(chip.tone).toBe("disabled");
      expect(chip.title).toBe(NO_ROOM_REASON);
    }
    expect(view.whitelist.face).toBe("NO ROOM");
  });

  it("tells the composer to warn, in the canonical words", () => {
    expect(deriveStripView(baseState({ roomProven: false })).composerWarning)
      .toBe(UNPROVEN_COMPOSER_PLACEHOLDER);
    expect(deriveStripView(baseState()).composerWarning).toBeNull();
  });

  it("keeps per-control reasons when the room IS proven but a control has no backend", () => {
    const view = deriveStripView(baseState({
      whitelist: { available: false, reason: "The whitelist roster lives in the OSL hub for now", roster: null },
    }));
    expect(view.whitelist.disabled).toBe(true);
    expect(view.whitelist.title).toContain("lives in the OSL hub");
    // The rest of the strip stays live: honesty is per control, not all-or-nothing.
    expect(view.timer.disabled).toBe(false);
    expect(view.eye.disabled).toBe(false);
  });

  it("never leaves a greyed control unexplained", () => {
    const view = deriveStripView(baseState({ roomProven: false }));
    for (const chip of [view.home, view.plan, view.quick, view.burn, view.whitelist, view.timer, view.once, view.lock, view.eye]) {
      if (chip.disabled) expect(chip.title.length).toBeGreaterThan(0);
    }
  });
});

describe("toggle-to-reveal derivation", () => {
  it("has a persistent revealed state that a second activation can turn off", () => {
    const rest = deriveStripView(baseState());
    expect(rest.eye.pressed).toBe(false);
    expect(rest.eye.tone).toBe("danger");
    const revealed = deriveStripView(baseState({ reveal: { available: true, revealed: true } }));
    expect(revealed.eye.pressed).toBe(true);
    expect(revealed.eye.tone).toBe("safe");
    expect(revealed.eye.title).toContain("click to restore the cover text");
    expect(rest.eye.title).toContain("Click to see the real message");
  });

  it("an unproven room disables the eye even when reveal was toggled on", () => {
    const view = deriveStripView(baseState({ roomProven: false, reveal: { available: true, revealed: true } }));
    expect(view.eye.disabled).toBe(true);
  });
});

describe("strip.ts interaction contract (source audit — no DOM in this package)", () => {
  const source = readRelative("./strip.ts");

  it("uses click as the only reveal activation and toggles the current state", () => {
    expect(source).toContain('eyeChip.addEventListener("click", () => {');
    expect(source).toContain("actions.onRevealToggle(!view.eye.pressed)");
    expect(source).not.toContain("onRevealHold");
    expect(source).not.toContain("Hold to see the real message");
    expect(source).not.toContain('addEventListener("pointerdown"');
    expect(source).not.toContain('addEventListener("pointerup"');
    expect(source).not.toContain('addEventListener("pointercancel"');
  });

  it("fails closed when an active reveal becomes unavailable", () => {
    expect(source).toContain("if (next.reveal.revealed && view.eye.disabled) actions.onRevealToggle(false);");
  });

  it("uses aria-disabled, never the disabled attribute, so greyed controls keep their hover tooltip", () => {
    expect(source).toContain('element.setAttribute("aria-disabled", "true")');
    expect(source).not.toMatch(/\.disabled\s*=\s*true/);
  });
});

describe("chip facts", () => {
  it("renders the canonical timer faces", () => {
    expect(stripTimerFace(3_600)).toBe("1h");
    expect(stripTimerFace(86_400)).toBe("1d");
    expect(stripTimerFace(259_200)).toBe("3d");
    expect(stripTimerFace(604_800)).toBe("7d");
    expect(stripTimerFace(90_000)).toBe("1d1h");
    expect(stripTimerFace(null)).toBe("OFF");
    expect(stripTimerWords(90_000)).toBe("1 day 1 hour");
  });

  it("derives whitelist allowed/total and the armed ONCE face", () => {
    const view = deriveStripView(baseState());
    expect(view.whitelist.face).toBe("1/3");
    expect(view.timer.face).toBe("1d");
    expect(view.timer.tone).toBe("timer");
    expect(view.once.face).toBe("ONCE OFF");
    const armed = deriveStripView(baseState({ once: { available: true, armed: true, seconds: 30 } }));
    expect(armed.once.face).toBe("ONCE 30s");
    expect(armed.once.tone).toBe("timer");
  });

  it("shows the plan chip grey when free and purple when pro", () => {
    expect(deriveStripView(baseState()).plan).toMatchObject({ face: "FREE", tone: "muted" });
    expect(deriveStripView(baseState({ plan: "pro" })).plan).toMatchObject({ face: "PRO", tone: "pro" });
  });

  it("shows the lock green-closed only when protection is on", () => {
    expect(deriveStripView(baseState()).lock.tone).toBe("safe");
    expect(deriveStripView(baseState({ lock: { state: "off", toggle: { available: true } } })).lock.tone).toBe("danger");
    const unreachable = deriveStripView(baseState({ lock: { state: "unreachable", toggle: { available: false, reason: "x" } } }));
    expect(unreachable.lock.tone).toBe("danger");
    expect(unreachable.lock.title).toContain("blocked, not downgraded");
  });

  it("styles chip faces as the mono status style", () => {
    const css = readRelative("./strip.css");
    const faceRule = css.slice(css.indexOf(".osl-strip__face"));
    expect(faceRule).toContain("--osl-strip-mono");
    expect(css).toContain("--osl-strip-mono: Consolas, ui-monospace, monospace;");
    // The canon faces are literal: "1d" stays lowercase, so no transform here.
    expect(faceRule.slice(0, faceRule.indexOf("}"))).not.toContain("uppercase");
  });
});

describe("token conformance", () => {
  it("strip.css uses only colours osl-tokens.ts blesses", () => {
    const css = readRelative("./strip.css");
    const blessed = new Set(allColours.map((colour) => colour.toLowerCase()));
    const hexes = [...css.matchAll(/#[0-9a-fA-F]{6}\b/g)];
    expect(hexes.length).toBeGreaterThan(10);
    for (const [hex] of hexes) {
      expect(blessed.has(hex.toLowerCase()), `${hex} is not an OSL colour`).toBe(true);
    }
    // rgba() is allowed only as an alpha form of a blessed colour, or pure
    // black for the modal drop shadow (the one shadow the design permits).
    for (const [, r, g, b] of css.matchAll(/rgba\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/g)) {
      const hex = `#${[r, g, b].map((part) => Number(part).toString(16).padStart(2, "0")).join("")}`;
      expect(blessed.has(hex) || hex === "#000000", `rgba base ${hex} is not an OSL colour`).toBe(true);
    }
  });

  it("the strip band stays out of the natively sized composer window", () => {
    const css = readRelative("./strip.css");
    expect(css).toContain(".osl-strip-band { display: none; }");
    expect(css).toContain("@media (min-height: 102px)");
    const overlayHtml = readRelative("../overlay.html");
    expect(overlayHtml).toContain('<div id="osl-strip" class="osl-strip-band"></div>');
  });
});
