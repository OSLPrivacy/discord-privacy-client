import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * The gate that was missing.
 *
 * 94 class names were being written into shipping markup with no rule anywhere
 * in the shipped sheet, and four of the six IA destinations plus the whole OSL
 * Mail client rendered as raw HTML: <strong> and <span> ran together into
 * "This computerAvailable", the Inbox filter tabs fell back to the native OS
 * button with its 3D bevel, and sixteen accounts stacked in one column. Nothing
 * failed, because nothing was looking.
 *
 * It cannot be fixed at runtime either: the shipped CSP is `style-src 'self'`
 * (apps/osl-hub/tauri.conf.json), so a runtime <style> element and an inline
 * style= attribute are both dropped. A rule that is not in a sheet does not
 * exist, which is exactly why this has to be checked against the sheets.
 *
 * The check is deliberately about ELEMENTS, not class names. A class that is
 * only a query hook on an element another class already paints is not a defect;
 * an element where NO class resolves to a rule is one, because that element is
 * being painted by nothing at all.
 */

const SHEETS = ["./styles.css", "./local-protected-sheet.css"] as const;

/**
 * Markup that ships inside the hub window. main.ts renders the chrome and every
 * destination; these two render whole routes it embeds. Modules that no shipping
 * entrypoint imports (osl-notes.ts, osl-office.ts) are deliberately absent --
 * they carry their own unstyled classes and are not on screen.
 */
const MARKUP = ["./main.ts", "./osl-mail-view.ts", "./osl-chats-view.ts"] as const;

interface ClassAttribute {
  readonly tokens: readonly string[];
  /** True when the attribute is partly built from an interpolation, so the
   *  static token list cannot be the whole story. */
  readonly dynamic: boolean;
  readonly source: string;
}

/**
 * Every class attribute in a template-literal HTML string, with the tokens it
 * can be shown to emit.
 *
 * Interpolations are walked with a brace counter rather than a regex, because a
 * class attribute here routinely contains `${x ? "selected" : ""}` -- quotes
 * inside the attribute that do not end it. A quoted literal inside an
 * interpolation is a class the attribute can emit, unless it is an operand of a
 * comparison, in which case it is a value being tested (`${mode === "osl" ?`)
 * and emits nothing.
 */
export function classAttributes(source: string): ClassAttribute[] {
  const found: ClassAttribute[] = [];
  const opener = /class\s*=\s*(["'])/gu;
  let match: RegExpExecArray | null;
  while ((match = opener.exec(source))) {
    const quote = match[1];
    let index = match.index + match[0].length;
    const start = index;
    let depth = 0;
    let plain = "";
    let current = "";
    const interpolations: string[] = [];
    while (index < source.length) {
      const character = source[index];
      if (depth === 0 && character === quote) break;
      if (character === "$" && source[index + 1] === "{") { depth = 1; index += 2; current = ""; continue; }
      if (depth > 0) {
        if (character === "{") depth += 1;
        else if (character === "}") {
          depth -= 1;
          if (depth === 0) { interpolations.push(current); plain += " "; index += 1; continue; }
        }
        current += character;
      } else {
        plain += character;
      }
      index += 1;
    }
    const tokens = new Set<string>();
    for (const token of plain.split(/\s+/u)) if (/^[-_a-zA-Z][\w-]*$/u.test(token)) tokens.add(token);
    for (const chunk of interpolations) {
      for (const literal of chunk.matchAll(/(["'])((?:(?!\1)[^\\])*)\1/gu)) {
        const before = chunk.slice(0, literal.index).trimEnd();
        const after = chunk.slice(literal.index + literal[0].length).trimStart();
        if (/[=!]==?$/u.test(before) || /^[=!]==?/u.test(after)) continue;
        for (const token of literal[2].split(/\s+/u)) if (/^[-_a-zA-Z][\w-]*$/u.test(token)) tokens.add(token);
      }
    }
    found.push({ tokens: [...tokens], dynamic: interpolations.length > 0, source: source.slice(start, index) });
    opener.lastIndex = index;
  }
  return found;
}

/**
 * Class names some rule actually paints -- the SUBJECT of a selector, not any
 * class the selector happens to mention.
 *
 * The distinction is the whole point. `.connections-grid > section { ... }`
 * mentions `.connections-grid` while painting only its children, so counting
 * mentions would call the grid covered at the exact moment its own
 * `display: grid` went missing and sixteen accounts fell back into one column.
 * Only the last compound of a selector describes the element the rule paints.
 */
export function styledClassNames(css: string): Set<string> {
  const declarations = css.replace(/\/\*[\s\S]*?\*\//gu, "");
  const names = new Set<string>();
  let cursor = 0;
  let depth = 0;
  for (let index = 0; index < declarations.length; index += 1) {
    const character = declarations[index];
    if (character === "}") { depth = Math.max(0, depth - 1); cursor = index + 1; continue; }
    if (character !== "{") continue;
    const prelude = declarations.slice(cursor, index).trim();
    cursor = index + 1;
    depth += 1;
    // `@media`/`@supports` open a block whose prelude is a condition, not a
    // selector; the selectors inside are read on their own next time round.
    if (prelude.startsWith("@")) continue;
    for (const selector of prelude.split(",")) {
      const subject = selector.trim().split(/[\s>+~]+/u).filter(Boolean).pop() ?? "";
      for (const match of subject.matchAll(/\.(-?[_a-zA-Z][\w-]*)/gu)) names.add(match[1]);
    }
  }
  return names;
}

/**
 * Containers that are painted only through their children, on purpose.
 *
 * Each is a <details> whose own box is meant to be invisible -- the summary and
 * the opened panel carry the whole appearance -- so the check below would
 * otherwise report an element that renders correctly. Every entry has to be a
 * container whose children ARE painted; anything else belongs in the sheet, not
 * here. Keep this list short: it is the one place this gate can be silenced.
 */
const PAINTED_THROUGH_CHILDREN = new Set([
  "recovery-account-details",
  "friend-security",
  "loading-host",
]);

/** Class attributes where not one token resolves to a rule that paints it. */
export function unpaintedElements(markup: readonly string[], css: string): string[] {
  const styled = styledClassNames(css);
  const offenders: string[] = [];
  for (const source of markup) {
    for (const attribute of classAttributes(source)) {
      if (attribute.tokens.length === 0) continue;
      if (attribute.tokens.some((token) => styled.has(token) || PAINTED_THROUGH_CHILDREN.has(token))) continue;
      offenders.push(attribute.source.trim().slice(0, 120));
    }
  }
  return offenders;
}

const css = SHEETS.map((sheet) => readFileSync(new URL(sheet, import.meta.url), "utf8")).join("\n");
const markup = MARKUP.map((file) => readFileSync(new URL(file, import.meta.url), "utf8"));

describe("shipped markup is painted by the shipped sheet", () => {
  it("leaves no element whose every class is unknown to the sheet", () => {
    expect(unpaintedElements(markup, css)).toEqual([]);
  });

  /**
   * The check above is only worth having if it can fail. Delete one rule and it
   * has to notice -- and notice THAT rule, not merely go red. A gate that
   * cannot be shown to bite is decoration.
   */
  it("fails when a rule is deleted", () => {
    const withoutInboxTabs = css.replace(/\.inbox-filter-tabs[^{]*\{[^}]*\}/gu, "");
    expect(withoutInboxTabs).not.toEqual(css);
    const offenders = unpaintedElements(markup, withoutInboxTabs);
    expect(offenders).not.toEqual([]);
    expect(offenders.join("\n")).toContain("inbox-filter-tabs");

    // ...and it is not simply always red: the same markup against the real
    // sheet is clean, so the failure above came from the deletion.
    expect(unpaintedElements(markup, css)).toEqual([]);
  });

  /**
   * Element-level coverage cannot see a rule that targets a child by tag, and
   * those are exactly the rules that failed here: the Burn reach list is
   * <li><strong>/<span>/<p> with no classes at all, and it ran together into
   * "This computerAvailable" while its container was painted the whole time.
   * These are named one by one because nothing else can find them.
   */
  it("keeps the layout rules that no class token can vouch for", () => {
    const required = [
      // Burn: the five reach surfaces, the highest-consequence copy in the app.
      ".burn-guarantees > ul > li",
      ".burn-guarantees > ul > li > strong",
      ".burn-guarantees > ul > li > span",
      ".burn-guarantees > ul > li > p",
      // Inbox: the tabs that fell back to native OS buttons.
      ".inbox-filter-tabs > button",
      ".inbox-surface-card > strong",
      ".inbox-surface-card > small",
      // Connections: sixteen accounts in one column.
      ".connection-row > div",
      ".connection-row > div > span",
      // Activity: "0Need attention", "LocalProof source".
      ".activity-proof-summary > article",
      ".activity-proof-summary > article > strong",
      ".activity-proof-summary > article > span",
      // Identity storage: a warning that read as one run-on line.
      ".storage-protection-status > strong",
      ".storage-protection-status > small",
    ];
    for (const selector of required) expect(css).toContain(`${selector} `);
  });

  /**
   * The reach state of Burn and the tone of a status chip are safety claims, so
   * they are stated in words first. These assert the word is never alone in
   * carrying the state to the sheet, and never replaced by the colour.
   */
  it("keeps state legible without colour", () => {
    const main = markup[0];
    expect(main).toContain('data-burn-reach="${escapeHtml(item.state)}"');
    expect(main).toContain("<span>${escapeHtml(stateLabel(item.state))}</span>");
    expect(css).toContain('.burn-guarantees > ul > li[data-burn-reach="not_possible"]');

    // A status chip with no resolved tone stays neutral rather than claiming a
    // success it has not verified.
    expect(css).toMatch(/\.status-tag\s*\{[^}]*color:\s*var\(--muted\)/su);
    expect(css).toMatch(/\.status-tag\.danger\s*\{[^}]*color:\s*var\(--danger\)/su);
    expect(css).toMatch(/\.status-tag\.warn\s*\{[^}]*color:\s*var\(--warn\)/su);
    expect(main).toMatch(/\["danger", \[[^\]]*"refused"/u);
    expect(main).toMatch(/\["danger", \[[^\]]*"needs attention"/u);
  });

  /**
   * `* { border-radius: 0 !important; }` is a deliberate square-corner decision
   * (home-layout.test.ts pins it). At equal specificity `!important` made it
   * beat every intentional disc in the sheet, so the Home avatar rendered as a
   * square tile. The exception has to outrank it and stay about discs only.
   */
  it("keeps discs round without softening a single corner", () => {
    expect(css).toContain("* { border-radius: 0 !important; }");
    const exception = css.replace(/\/\*[\s\S]*?\*\//gu, "").match(/([^{}]*)\{\s*border-radius:\s*50% !important;\s*\}/u);
    expect(exception).not.toBeNull();
    const selectors = (exception?.[1] ?? "").split(",").map((entry) => entry.trim()).filter(Boolean);
    expect(selectors).toContain(".home-profile-dock");
    expect(selectors).toContain(".dot");
    // Every element in the exception is a disc: it is round in the sheet's own
    // words elsewhere, at 50%. Nothing with an actual corner may be smuggled in.
    for (const selector of selectors) {
      const escaped = selector.replace(/[.*+?^${}()|[\]\\]/gu, "\\$&");
      expect(css).toMatch(new RegExp(`${escaped}[^{}]*\\{[^}]*border-radius:\\s*50%`, "u"));
    }
    // Nothing may re-round a corner with var(--radius) under !important.
    expect(css).not.toMatch(/border-radius:\s*var\(--radius\)\s*!important/u);
  });
});
