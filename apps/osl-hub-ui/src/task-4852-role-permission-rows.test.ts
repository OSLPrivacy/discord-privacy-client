import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ENFORCEMENT_SENTENCES,
  ENFORCEMENT_TAGS,
  ROLE_PERMISSION_ROWS,
  ROLE_PERMISSION_SECTIONS,
  checkRolePermissionScreenDump,
  parsePermissionCatalogue,
  rolePermissionRowMarkup,
  rolePermissionScreenDump,
  rolePermissionScreenMarkup,
  toggleRolePermission,
} from "./role-permission-rows";
import type { EnforcementTag, RolePermissionScreenState } from "./role-permission-rows";

const SRC_DIR = path.dirname(fileURLToPath(import.meta.url));

const KEY_SENTENCE = "Not a rule. They do not have the key.";
const RELAY_SENTENCE = "OSL's relay refuses it. It still cannot read what you write.";
const TRUST_SENTENCE = "A modified app could ignore this. Everyone else's app will still hide it.";

const MODERATOR: RolePermissionScreenState = {
  roleName: "Moderator",
  allowed: [
    "read a text channel",
    "read channel history",
    "send a message",
    "attach pictures",
    "timeout a member",
    "request clients hide a delivered message",
    "pin or unpin a message",
    "join voice",
  ],
};

function countOccurrences(haystack: string, needle: string): number {
  let count = 0;
  let from = 0;
  for (;;) {
    const at = haystack.indexOf(needle, from);
    if (at === -1) return count;
    count += 1;
    from = at + needle.length;
  }
}

describe("TASK 4852 - the three enforcement explanations", () => {
  it("states the three sentences word for word", () => {
    expect(ENFORCEMENT_SENTENCES.KEY).toBe(KEY_SENTENCE);
    expect(ENFORCEMENT_SENTENCES.RELAY).toBe(RELAY_SENTENCE);
    expect(ENFORCEMENT_SENTENCES.TRUST).toBe(TRUST_SENTENCE);
    expect(ENFORCEMENT_TAGS).toEqual(["KEY", "RELAY", "TRUST"]);
  });

  it("carries TASK 4851's 40 rows and 40 tags, one tag per row", () => {
    expect(ROLE_PERMISSION_ROWS).toHaveLength(40);
    expect(ROLE_PERMISSION_SECTIONS).toHaveLength(7);
    expect(ROLE_PERMISSION_ROWS.filter((row) => ENFORCEMENT_TAGS.includes(row.tag))).toHaveLength(40);
    for (const row of ROLE_PERMISSION_ROWS) {
      expect(row.sentence).toBe(ENFORCEMENT_SENTENCES[row.tag]);
    }
  });

  it("refuses a catalogue row that has no tag", () => {
    expect(() => parsePermissionCatalogue("talking\n- send a message\n"))
      .toThrowError(/row has no enforcement tag: send a message/u);
    expect(() => parsePermissionCatalogue("talking\n- send a message `MAYBE`\n"))
      .toThrowError(/unknown enforcement tag `MAYBE`/u);
  });
});

describe("TASK 4852 - every drawn row shows its tag and its sentence", () => {
  it("draws both on a single row, whether the permission is on or off", () => {
    const row = ROLE_PERMISSION_ROWS.find((candidate) => candidate.tag === "TRUST");
    if (!row) throw new Error("no TRUST row in the catalogue");
    for (const allowed of [true, false]) {
      const markup = rolePermissionRowMarkup(row, allowed).replace(/&#39;/gu, "'");
      expect(markup).toContain(`data-enforcement-tag="TRUST"`);
      expect(markup).toContain(">TRUST<");
      expect(markup).toContain(TRUST_SENTENCE);
    }
  });

  it("shows 40 tags beside 40 rows on the role screen", () => {
    const markup = rolePermissionScreenMarkup(MODERATOR);
    expect(countOccurrences(markup, `data-permission-row="`)).toBe(40);
    expect(countOccurrences(markup, `class="role-permission-tag" data-enforcement-tag=`)).toBe(43);
    expect(countOccurrences(markup, `class="role-permission-sentence" data-enforcement-sentence=`)).toBe(43);

    const report = checkRolePermissionScreenDump(rolePermissionScreenDump(MODERATOR));
    expect(report.rows).toBe(40);
    expect(report.tags).toBe(40);
    expect(report.sentences).toBe(40);
    expect(report.legendSentences).toBe(3);
    expect(report.sentenceCounts.KEY + report.sentenceCounts.RELAY + report.sentenceCounts.TRUST).toBe(40);
    expect(report.roleName).toBe("Moderator");
  });

  it("shows the three exact sentences on screen", () => {
    const markup = rolePermissionScreenMarkup(MODERATOR);
    const text = markup.replace(/&#39;/gu, "'");
    for (const sentence of [KEY_SENTENCE, RELAY_SENTENCE, TRUST_SENTENCE]) {
      expect(text).toContain(sentence);
    }
  });

  it("keeps the tag and the sentence when a switch is flipped", () => {
    const flipped = toggleRolePermission(MODERATOR, "ban a member");
    expect(flipped.allowed).toContain("ban a member");
    const report = checkRolePermissionScreenDump(rolePermissionScreenDump(flipped));
    expect(report.rows).toBe(40);
    expect(report.sentences).toBe(40);
  });
});

describe("TASK 4852 - the screen check goes red when a sentence is deleted", () => {
  const dump = rolePermissionScreenDump(MODERATOR);

  for (const tag of ["KEY", "RELAY", "TRUST"] as EnforcementTag[]) {
    it(`refuses a screen with the ${tag} sentence deleted everywhere`, () => {
      const sentence = ENFORCEMENT_SENTENCES[tag];
      const without = dump.split("\n").map((line) => line.replace(sentence, "")).join("\n");
      expect(() => checkRolePermissionScreenDump(without))
        .toThrowError(new RegExp(`the screen never shows the ${tag} sentence`, "u"));
      expect(() => checkRolePermissionScreenDump(without))
        .toThrowError(/shows no enforcement sentence/u);
    });
  }

  it("refuses a screen with one single row's sentence deleted", () => {
    const without = dump
      .split("\n")
      .map((line) => (line.startsWith("row: mention everyone | TRUST") ? "row: mention everyone | TRUST | " : line))
      .join("\n");
    expect(() => checkRolePermissionScreenDump(without))
      .toThrowError(/row shows no enforcement sentence: mention everyone `TRUST`/u);
  });

  it("refuses a screen where a sentence was swapped for the wrong class", () => {
    const swapped = dump
      .split("\n")
      .map((line) => (line.startsWith("row: join voice | RELAY")
        ? `row: join voice | RELAY | ${KEY_SENTENCE}`
        : line))
      .join("\n");
    expect(() => checkRolePermissionScreenDump(swapped))
      .toThrowError(/row shows the wrong enforcement sentence: join voice `RELAY`/u);
  });

  it("refuses a screen with a row but no tag", () => {
    const untagged = dump
      .split("\n")
      .map((line) => (line.startsWith("row: send a message | KEY") ? "row: send a message |  | " : line))
      .join("\n");
    expect(() => checkRolePermissionScreenDump(untagged))
      .toThrowError(/row shows no enforcement tag: send a message/u);
  });

  it("refuses a screen that drops a row entirely", () => {
    const short = dump.split("\n").filter((line) => !line.startsWith("row: ban a member")).join("\n");
    expect(() => checkRolePermissionScreenDump(short))
      .toThrowError(/expected 40 permission rows on screen, found 39/u);
  });
});

describe("TASK 4852 - one place draws a permission row", () => {
  it("has no other module drawing catalogue rows on its own", () => {
    const owner = "role-permission-rows.ts";
    const phrases = ROLE_PERMISSION_ROWS.map((row) => row.words);
    const offenders: string[] = [];

    for (const name of readdirSync(SRC_DIR)) {
      if (!name.endsWith(".ts") || name === owner) continue;
      if (name === "task-4852-role-permission-rows.test.ts") continue;
      const source = readFileSync(path.join(SRC_DIR, name), "utf8");
      const hits = phrases.filter((phrase) => source.includes(phrase));
      // Three or more catalogue phrases in one file means that file is listing
      // permissions; incidental wording ("send a message") is not enough.
      if (hits.length < 3) continue;
      if (source.includes("./role-permission-rows")) continue;
      offenders.push(`${name} (${hits.length} rows)`);
    }

    expect(offenders).toEqual([]);
  });
});
