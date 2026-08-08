import "./role-permission-rows.css";
import CATALOGUE_TEXT from "./fixtures/permission-catalogue.txt?raw";

/**
 * TASK 4852 - the three enforcement explanations, on screen beside every role
 * permission row.
 *
 * TASK 4851 sorted all 40 role permissions into three classes and tagged each
 * row `KEY`, `RELAY` or `TRUST`. A tag on its own is jargon, so this module is
 * the one place a permission row is drawn, and it cannot draw a row without
 * drawing both the tag and the honest sentence for that class:
 *
 *   KEY   - it is not a rule at all. The other side does not hold the key, so
 *           there is nothing to enforce and nothing to break.
 *   RELAY - OSL's own relay refuses the request. The relay is still blind: it
 *           refuses without ever reading what was written.
 *   TRUST - honest apps obey it. A modified app can ignore it, and everyone
 *           else's app still hides it.
 *
 * The 40 rows and their tags are not retyped here. `fixtures/permission-catalogue.txt`
 * is the exact output of TASK 4851's `osl-permission-catalogue print`, and
 * `crates/ipc/tests/task4852_role_screen_catalogue.rs` fails if that file ever
 * stops matching the Rust catalogue.
 */
export type EnforcementTag = "KEY" | "RELAY" | "TRUST";

export const ENFORCEMENT_TAGS: readonly EnforcementTag[] = ["KEY", "RELAY", "TRUST"];

/**
 * The three sentences, word for word. They are what the screen promises about
 * enforcement, so they are stated once and read from here everywhere.
 */
export const ENFORCEMENT_SENTENCES: Readonly<Record<EnforcementTag, string>> = {
  KEY: "Not a rule. They do not have the key.",
  RELAY: "OSL's relay refuses it. It still cannot read what you write.",
  TRUST: "A modified app could ignore this. Everyone else's app will still hide it.",
};

/** The short label above each sentence in the legend, so the tag has a name as well as a rule. */
export const ENFORCEMENT_TITLES: Readonly<Record<EnforcementTag, string>> = {
  KEY: "Held by the key",
  RELAY: "Refused by the relay",
  TRUST: "Obeyed by honest apps",
};

export interface RolePermissionRow {
  section: string;
  words: string;
  tag: EnforcementTag;
  /** Always `ENFORCEMENT_SENTENCES[tag]`. Carried on the row so a row cannot be shown without it. */
  sentence: string;
}

export function isEnforcementTag(value: string): value is EnforcementTag {
  return (ENFORCEMENT_TAGS as readonly string[]).includes(value);
}

export function enforcementSentence(tag: EnforcementTag): string {
  return ENFORCEMENT_SENTENCES[tag];
}

/**
 * Reads TASK 4851's catalogue text: a section name on its own line, then its
 * rows as "- words `TAG`". A row with no tag, an unknown tag, or a row before
 * any section name is a parse failure rather than a silently dropped row.
 */
export function parsePermissionCatalogue(text: string): RolePermissionRow[] {
  const rows: RolePermissionRow[] = [];
  let section = "";
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (line === "") continue;
    if (!line.startsWith("- ")) {
      section = line;
      continue;
    }
    const body = line.slice(2).trim();
    if (section === "") throw new Error(`permission catalogue: row before any section name: ${body}`);
    const match = /^(.*) `([A-Z]+)`$/u.exec(body);
    if (!match) throw new Error(`permission catalogue: row has no enforcement tag: ${body}`);
    const words = match[1].trim();
    const tag = match[2];
    if (!isEnforcementTag(tag)) {
      throw new Error(`permission catalogue: row has unknown enforcement tag \`${tag}\`: ${words}`);
    }
    rows.push({ section, words, tag, sentence: ENFORCEMENT_SENTENCES[tag] });
  }
  return rows;
}

/** All 40 rows of TASK 4851's catalogue, in catalogue order. */
export const ROLE_PERMISSION_ROWS: readonly RolePermissionRow[] = parsePermissionCatalogue(CATALOGUE_TEXT);

/** The 7 section names, in catalogue order. */
export const ROLE_PERMISSION_SECTIONS: readonly string[] = ROLE_PERMISSION_ROWS
  .map((row) => row.section)
  .filter((section, index, all) => all.indexOf(section) === index);

export interface RolePermissionScreenState {
  /** The role being looked at, e.g. "Moderator". */
  roleName: string;
  /** Row words that are switched on for this role. Anything not listed is off. */
  allowed: readonly string[];
}

export function isRowAllowed(state: RolePermissionScreenState, words: string): boolean {
  return state.allowed.includes(words);
}

export function toggleRolePermission(
  state: RolePermissionScreenState,
  words: string,
): RolePermissionScreenState {
  if (!ROLE_PERMISSION_ROWS.some((row) => row.words === words)) return state;
  const allowed = isRowAllowed(state, words)
    ? state.allowed.filter((allowedWords) => allowedWords !== words)
    : ROLE_PERMISSION_ROWS
      .map((row) => row.words)
      .filter((rowWords) => rowWords === words || state.allowed.includes(rowWords));
  return { ...state, allowed };
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    '"': "&quot;",
    "'": "&#39;",
  })[character] ?? character);
}

function unescapeHtml(value: string): string {
  return value
    .replace(/&#39;/gu, "'")
    .replace(/&quot;/gu, '"')
    .replace(/&gt;/gu, ">")
    .replace(/&lt;/gu, "<")
    .replace(/&amp;/gu, "&");
}

/**
 * The one way a role permission row is drawn. The tag and the sentence are not
 * optional arguments and there is no branch that leaves either out: a caller
 * that shows a row shows both, or does not show a row.
 */
export function rolePermissionRowMarkup(row: RolePermissionRow, allowed: boolean): string {
  const sentence = ENFORCEMENT_SENTENCES[row.tag];
  return `<li class="role-permission-row${allowed ? " allowed" : ""}" data-permission-row="${escapeHtml(row.words)}" data-enforcement="${row.tag}">
      <span class="role-permission-words">${escapeHtml(row.words)}</span>
      <span class="role-permission-switch" data-permission-switch role="switch" aria-checked="${allowed ? "true" : "false"}" tabindex="0">${allowed ? "On" : "Off"}</span>
      <span class="role-permission-tag" data-enforcement-tag="${row.tag}">${row.tag}</span>
      <span class="role-permission-sentence" data-enforcement-sentence="${row.tag}">${escapeHtml(sentence)}</span>
    </li>`;
}

function legendMarkup(): string {
  const items = ENFORCEMENT_TAGS.map((tag) => `<li class="role-permission-legend-item" data-enforcement-legend="${tag}">
        <span class="role-permission-tag" data-enforcement-tag="${tag}">${tag}</span>
        <span class="role-permission-legend-title">${escapeHtml(ENFORCEMENT_TITLES[tag])}</span>
        <span class="role-permission-sentence" data-enforcement-sentence="${tag}">${escapeHtml(ENFORCEMENT_SENTENCES[tag])}</span>
      </li>`).join("");
  return `<ul class="role-permission-legend" data-enforcement-legend-list>${items}</ul>`;
}

function sectionMarkup(section: string, state: RolePermissionScreenState): string {
  const rows = ROLE_PERMISSION_ROWS
    .filter((row) => row.section === section)
    .map((row) => rolePermissionRowMarkup(row, isRowAllowed(state, row.words)))
    .join("");
  return `<section class="role-permission-section" data-permission-section="${escapeHtml(section)}">
      <h2 class="role-permission-section-name">${escapeHtml(section)}</h2>
      <ul class="role-permission-rows">${rows}</ul>
    </section>`;
}

export function rolePermissionScreenMarkup(state: RolePermissionScreenState): string {
  const sections = ROLE_PERMISSION_SECTIONS.map((section) => sectionMarkup(section, state)).join("");
  return `<section class="role-permission-screen" data-role-permission-screen data-role-name="${escapeHtml(state.roleName)}" aria-labelledby="role-permission-heading">
    <h1 id="role-permission-heading" tabindex="-1">What ${escapeHtml(state.roleName)} can do</h1>
    <p class="role-permission-intro">Every switch below says who enforces it, in the same words every time. Three things enforce a permission in OSL, and only one of them is a rule anyone could break.</p>
    ${legendMarkup()}
    <p class="role-permission-count" data-permission-count>${ROLE_PERMISSION_ROWS.length} permissions · ${ROLE_PERMISSION_ROWS.length} enforcement tags.</p>
    ${sections}
  </section>`;
}

/* -------------------------------------------------------------------------
   The screen check.

   The check reads a dump of what the screen actually shows - one `row:` line
   per drawn row, carrying the row's words, its tag and the sentence printed
   next to it - so it fails on what is missing from the screen, not on what the
   source code happens to contain. The browser capture builds the same dump out
   of the live DOM, so both the source render and the real page go through this.
   ------------------------------------------------------------------------- */

export const ROLE_SCREEN_DUMP_MARK = "TASK4852 role screen dump";

export interface RolePermissionScreenReport {
  roleName: string;
  rows: number;
  tags: number;
  sentences: number;
  sentenceCounts: Record<EnforcementTag, number>;
  legendSentences: number;
}

function attribute(fragment: string, name: string): string | null {
  const match = new RegExp(`${name}="([^"]*)"`, "u").exec(fragment);
  return match ? unescapeHtml(match[1]) : null;
}

function innerText(fragment: string, selectorClass: string): string | null {
  const match = new RegExp(
    `<span class="${selectorClass}"[^>]*>([^<]*)</span>`,
    "u",
  ).exec(fragment);
  return match ? unescapeHtml(match[1]) : null;
}

/**
 * Turns rendered markup into the dump the check reads. A row whose sentence
 * element was removed produces a `row:` line with an empty sentence field,
 * which is exactly what the check refuses.
 */
export function rolePermissionScreenDumpFromMarkup(markup: string): string {
  const lines: string[] = [ROLE_SCREEN_DUMP_MARK];
  const roleName = attribute(markup, "data-role-name") ?? "";
  lines.push(`role: ${roleName}`);

  for (const item of markup.match(/<li class="role-permission-row[\s\S]*?<\/li>/gu) ?? []) {
    const words = innerText(item, "role-permission-words") ?? attribute(item, "data-permission-row") ?? "";
    const tag = innerText(item, "role-permission-tag") ?? "";
    const sentence = innerText(item, "role-permission-sentence") ?? "";
    lines.push(`row: ${words} | ${tag} | ${sentence}`);
  }

  for (const item of markup.match(/<li class="role-permission-legend-item[\s\S]*?<\/li>/gu) ?? []) {
    const tag = innerText(item, "role-permission-tag") ?? "";
    const sentence = innerText(item, "role-permission-sentence") ?? "";
    lines.push(`legend: ${tag} | ${sentence}`);
  }

  return `${lines.join("\n")}\n`;
}

export function rolePermissionScreenDump(state: RolePermissionScreenState): string {
  return rolePermissionScreenDumpFromMarkup(rolePermissionScreenMarkup(state));
}

/**
 * Refuses a role screen that is missing a tag, missing a sentence, or showing
 * the wrong sentence for a class. Every problem found is reported, not just the
 * first, so a red run names everything that has to be put back.
 */
export function checkRolePermissionScreenDump(dump: string): RolePermissionScreenReport {
  const problems: string[] = [];
  const lines = dump.split("\n").map((line) => line.trim()).filter((line) => line !== "");
  for (const tag of ENFORCEMENT_TAGS) {
    if (ENFORCEMENT_SENTENCES[tag].trim() === "") {
      problems.push(`the ${tag} enforcement sentence has been deleted`);
    }
  }
  const roleName = lines.find((line) => line.startsWith("role: "))?.slice(6) ?? "";
  const sentenceCounts: Record<EnforcementTag, number> = { KEY: 0, RELAY: 0, TRUST: 0 };
  let rows = 0;
  let tags = 0;
  let sentences = 0;
  let legendSentences = 0;

  const rowLines = lines
    .filter((line) => line.startsWith("row: "))
    .map((line) => {
      const parts = line.slice(5).split("|").map((part) => part.trim());
      return { words: parts[0] ?? "", tag: parts[1] ?? "", sentence: parts.slice(2).join(" | ").trim() };
    });

  for (const { words, tag, sentence } of rowLines) {
    rows += 1;
    if (words === "") problems.push(`row ${rows} shows no permission words`);
    if (tag === "") {
      problems.push(`row shows no enforcement tag: ${words}`);
      continue;
    }
    if (!isEnforcementTag(tag)) {
      problems.push(`row shows unknown enforcement tag \`${tag}\`: ${words}`);
      continue;
    }
    tags += 1;
    if (sentence === "") {
      problems.push(`row shows no enforcement sentence: ${words} \`${tag}\``);
      continue;
    }
    sentences += 1;
    if (sentence !== ENFORCEMENT_SENTENCES[tag]) {
      problems.push(
        `row shows the wrong enforcement sentence: ${words} \`${tag}\` expected "${ENFORCEMENT_SENTENCES[tag]}", found "${sentence}"`,
      );
      continue;
    }
    sentenceCounts[tag] += 1;
  }

  if (rows !== ROLE_PERMISSION_ROWS.length) {
    problems.push(`expected ${ROLE_PERMISSION_ROWS.length} permission rows on screen, found ${rows}`);
  }
  if (tags !== ROLE_PERMISSION_ROWS.length) {
    problems.push(`expected ${ROLE_PERMISSION_ROWS.length} enforcement tags on screen, found ${tags}`);
  }

  // The rows on screen have to be the catalogue's rows, tagged the catalogue's way.
  for (const [index, row] of ROLE_PERMISSION_ROWS.entries()) {
    const shown = rowLines[index];
    if (shown === undefined) {
      problems.push(`permission row missing from screen: ${row.words} \`${row.tag}\``);
      continue;
    }
    if (shown.words !== row.words) {
      problems.push(
        `permission row out of catalogue order: expected ${row.words}, found "${shown.words}"`,
      );
      continue;
    }
    if (shown.tag !== "" && shown.tag !== row.tag) {
      problems.push(
        `permission row carries the wrong tag: ${row.words} expected \`${row.tag}\`, found \`${shown.tag}\``,
      );
    }
  }

  const legendLines = lines.filter((line) => line.startsWith("legend: "));
  for (const tag of ENFORCEMENT_TAGS) {
    const sentence = ENFORCEMENT_SENTENCES[tag];
    if (!lines.some((line) => line.includes(sentence))) {
      problems.push(`the screen never shows the ${tag} sentence: "${sentence}"`);
    }
    if (legendLines.some((line) => line === `legend: ${tag} | ${sentence}`)) legendSentences += 1;
    else problems.push(`the legend never explains \`${tag}\`: "${sentence}"`);
  }

  if (roleName === "") problems.push("the screen does not name the role it is showing");

  if (problems.length > 0) {
    throw new Error(`role permission screen check failed: ${problems.join("; ")}`);
  }

  return { roleName, rows, tags, sentences, sentenceCounts, legendSentences };
}

export interface KeyEnforcementReport {
  roleName: string;
  rows: number;
  /** KEY-tagged rows in TASK 4851's catalogue. */
  catalogueKeyRows: number;
  /** KEY-tagged rows on screen showing the exact KEY words. */
  keyRowsShowing: number;
  /** RELAY or TRUST rows on screen showing the KEY words. Has to be 0. */
  nonKeyRowsShowingKey: number;
}

/**
 * TASK 5000 - the KEY words, checked on their own.
 *
 * KEY means cryptographic, unbypassable, and PRODUCT.txt fixes the copy for it:
 * "Not a rule. They do not have the key." This refuses a screen dump where any
 * KEY-tagged row does not show that exact sentence, where the number of KEY
 * rows showing it is not the number of KEY rows in the catalogue, or where a
 * single RELAY or TRUST row shows it. Rows are matched to the catalogue in
 * catalogue order, the same way the screen check matches them.
 */
export function checkKeyEnforcementWords(dump: string): KeyEnforcementReport {
  const problems: string[] = [];
  const keySentence = ENFORCEMENT_SENTENCES.KEY;
  const lines = dump.split("\n").map((line) => line.trim()).filter((line) => line !== "");
  const roleName = lines.find((line) => line.startsWith("role: "))?.slice(6) ?? "";
  const rowLines = lines
    .filter((line) => line.startsWith("row: "))
    .map((line) => {
      const parts = line.slice(5).split("|").map((part) => part.trim());
      return { words: parts[0] ?? "", tag: parts[1] ?? "", sentence: parts.slice(2).join(" | ").trim() };
    });

  const catalogueKeyRows = ROLE_PERMISSION_ROWS.filter((row) => row.tag === "KEY").length;
  let keyRowsShowing = 0;
  let nonKeyRowsShowingKey = 0;

  for (const [index, catalogueRow] of ROLE_PERMISSION_ROWS.entries()) {
    const shown = rowLines[index];
    if (shown === undefined) {
      if (catalogueRow.tag === "KEY") problems.push(`KEY row missing from screen: ${catalogueRow.words}`);
      continue;
    }
    if (shown.words !== catalogueRow.words) {
      problems.push(`permission row out of catalogue order: expected ${catalogueRow.words}, found "${shown.words}"`);
      continue;
    }
    if (catalogueRow.tag === "KEY") {
      if (shown.tag !== "KEY") {
        problems.push(`catalogue KEY row carries the wrong tag on screen: ${catalogueRow.words} found \`${shown.tag}\``);
        continue;
      }
      if (shown.sentence === keySentence) {
        keyRowsShowing += 1;
      } else {
        problems.push(`KEY row does not show the KEY words: ${catalogueRow.words} shows "${shown.sentence}"`);
      }
    } else if (shown.sentence === keySentence) {
      nonKeyRowsShowingKey += 1;
      problems.push(`${catalogueRow.tag} row shows the KEY words: ${catalogueRow.words}`);
    }
  }

  if (keyRowsShowing !== catalogueKeyRows) {
    problems.push(`expected ${catalogueKeyRows} KEY rows showing "${keySentence}", found ${keyRowsShowing}`);
  }
  if (nonKeyRowsShowingKey !== 0) {
    problems.push(`${nonKeyRowsShowingKey} RELAY or TRUST rows show the KEY words`);
  }

  if (problems.length > 0) {
    throw new Error(`KEY enforcement words check failed: ${problems.join("; ")}`);
  }

  return { roleName, rows: rowLines.length, catalogueKeyRows, keyRowsShowing, nonKeyRowsShowingKey };
}

export interface RolePermissionScreenHandle {
  state(): RolePermissionScreenState;
  dump(): string;
}

/** Mounts the screen. Flipping a switch re-renders through the same one row renderer. */
export function mountRolePermissionScreen(
  root: HTMLElement,
  state: RolePermissionScreenState,
): RolePermissionScreenHandle {
  let current = state;

  const render = (): void => {
    root.innerHTML = rolePermissionScreenMarkup(current);
  };

  const flip = (target: HTMLElement | null): void => {
    const row = target?.closest<HTMLElement>("[data-permission-row]");
    const words = row?.dataset.permissionRow;
    if (!words) return;
    current = toggleRolePermission(current, words);
    render();
  };

  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement | null;
    if (!target?.closest("[data-permission-switch]")) return;
    flip(target);
  });

  root.addEventListener("keydown", (event) => {
    if (event.key !== " " && event.key !== "Enter") return;
    const target = event.target as HTMLElement | null;
    if (!target?.closest("[data-permission-switch]")) return;
    event.preventDefault();
    flip(target);
  });

  render();

  return {
    state: () => current,
    dump: () => rolePermissionScreenDumpFromMarkup(root.innerHTML),
  };
}
