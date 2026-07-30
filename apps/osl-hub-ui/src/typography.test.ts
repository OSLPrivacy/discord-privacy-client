import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
const sheetStyles = readFileSync(new URL("./local-protected-sheet.css", import.meta.url), "utf8");
const overlayStyles = readFileSync(new URL("./overlay.css", import.meta.url), "utf8");
const allStyles = `${styles}\n${sheetStyles}\n${overlayStyles}`;

/**
 * The rule blocks whose job is to paint OSL's text directly onto Discord's, in
 * Discord's own typography: the protected composer draft once the native
 * composer capture is live, and the carrier-bound transcript rows.
 *
 * Everything else in every sheet is OSL's own chrome and stays inside OSL's own
 * design rules. These two surfaces cannot, by construction: their whole purpose
 * is to reproduce a measurement taken off Discord, and any project-wide
 * minimum, maximum or preferred value applied to them is a guarantee that OSL
 * will disagree with Discord whenever Discord disagrees with that value.
 */
const DISCORD_PAINTED_RULES =
  /[^{}]*(?:--carrier-bound|data-native-composer-capture="true"\]\s+\.draft-field\s+textarea)[^{}]*\{[^}]*\}/gu;

/**
 * The failed-send band: OSL-authored UI that happens to live inside the
 * protected composer window, and the one sanctioned exception to the 11px
 * floor. It is narrow on purpose -- the window is sized natively to Discord's
 * composer rectangle (measured live at 736x58) and cannot grow, so every row
 * the band takes is a row of the operator's own draft, and against Discord's
 * 14px message text 11px read as louder than the conversation it annotates.
 */
const SEND_FAILURE_RULES = /\.send-failure(?:__badge)?\s*\{[^}]*\}/gu;

/** OSL's own chrome: every sheet, minus the two surfaces above. */
const oslChrome = allStyles
  .replace(DISCORD_PAINTED_RULES, "")
  .replace(SEND_FAILURE_RULES, "");

/** Declarations only, so a rationale written in a comment never reads as code. */
function declarations(css: string): string {
  return css.replace(/\/\*[\s\S]*?\*\//gu, "");
}

describe("professional typography", () => {
  it("self-hosts Inter and reserves mono for machine data", () => {
    expect(styles).toContain('--font-ui: "Inter Variable"');
    expect(styles).toContain('--font-display: "Inter Variable"');
    expect(readFileSync(new URL("./main.ts", import.meta.url), "utf8")).toContain('@fontsource-variable/inter/wght.css');
    expect(styles).toContain('--font-mono: "Cascadia Mono"');
    expect(styles).toContain("font-family: var(--font-ui)");
    expect(styles).toContain(".identity-row small { font-family: var(--font-mono); }");
    expect(sheetStyles).toContain(".local-capsule-result textarea { font-family: var(--font-mono)");
  });

  it("does not regress to novelty fonts, tiny text, or fractional weights", () => {
    // The exception is stripped, the rule is not relaxed: OSL's own chrome
    // still has to clear 11px and still may not use novelty faces or the
    // in-between weights, in every sheet.
    expect(allStyles).not.toMatch(/Px437|Bahnschrift/u);
    expect(declarations(oslChrome)).not.toMatch(/font-size:\s*(?:[1-9]|10)px/u);
    expect(declarations(oslChrome)).not.toMatch(/font-weight:\s*(?:350|650|680|720|750|800)/u);
    // Scoping is only honest if it actually removed something and left the
    // rest: the composer draft and the carrier rows have to be findable, and
    // OSL's chrome has to still be here to be protected.
    expect(allStyles.match(DISCORD_PAINTED_RULES) ?? []).not.toHaveLength(0);
    expect(oslChrome).toContain(".composer-toolbar");
    expect(oslChrome).toContain("--font-ui");
  });

  it("puts no floor, ceiling or preferred value on the text painted over Discord", () => {
    // 1:1 means the measurement wins outright. Neither surface may pass a
    // measured metric through min()/max()/clamp()/calc(), and neither may take
    // a hardcoded value where a measured one exists.
    const painted = (allStyles.match(DISCORD_PAINTED_RULES) ?? []).map(declarations).join("\n");
    expect(painted).not.toMatch(/font-size:[^;]*(?:min|max|clamp|calc)\(/u);
    expect(painted).not.toMatch(/line-height:[^;]*(?:min|max|clamp|calc)\(/u);
    expect(painted).not.toMatch(/letter-spacing:[^;]*(?:min|max|clamp|calc)\(/u);
    expect(painted).not.toMatch(/font-weight:[^;]*(?:min|max|clamp|calc)\(/u);

    // The carrier rows consume all five measured properties, each with no
    // fallback at all: this rule is only live while every variable is set.
    const carrierRow = overlayStyles.slice(
      overlayStyles.indexOf(".osl-discord-transcript__row--carrier-bound {"),
    );
    const carrierBody = declarations(carrierRow.slice(0, carrierRow.indexOf("}")));
    expect(carrierBody).toContain("font-size: var(--osl-carrier-font-size);");
    expect(carrierBody).toContain("font-weight: var(--osl-carrier-font-weight);");
    expect(carrierBody).toContain("line-height: var(--osl-carrier-line-height);");
    expect(carrierBody).toContain("letter-spacing: var(--osl-carrier-letter-spacing);");

    // The composer draft consumes every property the capture actually carries.
    // Its fallbacks are per-declaration and are reached only when that exact
    // property was not measured -- never as a substitute for one that was.
    const composerRule = overlayStyles.slice(
      overlayStyles.indexOf(':root[data-native-composer-capture="true"] .draft-field textarea {'),
    );
    const composerBody = declarations(composerRule.slice(0, composerRule.indexOf("}")));
    expect(composerBody).toContain("font-size: var(--osl-native-edit-font-size,");
    expect(composerBody).toContain("font-weight: var(--osl-native-edit-font-weight,");
    expect(composerBody).toContain("line-height: var(--osl-native-edit-line-height,");
    expect(composerBody).toContain("font-family: var(--osl-native-edit-font-family,");
  });
});
