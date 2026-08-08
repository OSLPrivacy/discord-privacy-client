import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  ENFORCEMENT_SENTENCES,
  ROLE_PERMISSION_ROWS,
  checkKeyEnforcementWords,
  rolePermissionScreenDump,
} from "./role-permission-rows";
import type { RolePermissionScreenState } from "./role-permission-rows";

const SRC_DIR = path.dirname(fileURLToPath(import.meta.url));
/** TASK 4851's catalogue, byte for byte the output of `osl-permission-catalogue print`. */
const CATALOGUE_PATH = path.join(SRC_DIR, "fixtures", "permission-catalogue.txt");

/** PRODUCT.txt's exact copy for the KEY class: cryptographic, unbypassable. */
const KEY_SENTENCE = "Not a rule. They do not have the key.";
const RELAY_SENTENCE = "OSL's relay refuses it. It still cannot read what you write.";

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

/** The words of every row tagged `KEY` in the fixture, parsed straight off disk. */
function fixtureKeyRows(): string[] {
  const rows: string[] = [];
  for (const raw of readFileSync(CATALOGUE_PATH, "utf8").split("\n")) {
    const line = raw.trim();
    if (!line.startsWith("- ")) continue;
    const match = /^(.*) `([A-Z]+)`$/u.exec(line.slice(2).trim());
    if (match && match[2] === "KEY") rows.push(match[1].trim());
  }
  return rows;
}

describe("TASK 5000 - the KEY words on every KEY-tagged row", () => {
  it("states the KEY sentence word for word, PRODUCT.txt's copy", () => {
    expect(ENFORCEMENT_SENTENCES.KEY).toBe(KEY_SENTENCE);
  });

  it("counts the KEY rows in TASK 4851's catalogue", () => {
    const keyRows = fixtureKeyRows();
    expect(keyRows).toHaveLength(10);
    expect(ROLE_PERMISSION_ROWS.filter((row) => row.tag === "KEY")).toHaveLength(keyRows.length);
  });

  it("shows the exact KEY words on every KEY row, and on nothing else", () => {
    const dump = rolePermissionScreenDump(MODERATOR);
    const report = checkKeyEnforcementWords(dump);
    expect(report.catalogueKeyRows).toBe(10);
    expect(report.keyRowsShowing).toBe(10);
    expect(report.nonKeyRowsShowingKey).toBe(0);

    const rowLines = dump.split("\n").filter((line) => line.startsWith("row: "));
    expect(rowLines.filter((line) => line.includes(`| KEY | ${KEY_SENTENCE}`))).toHaveLength(10);
    expect(rowLines.filter((line) => /\| (RELAY|TRUST) \| /u.test(line) && line.includes(KEY_SENTENCE)))
      .toHaveLength(0);
  });
});

describe("TASK 5000 - the check goes red when the words move", () => {
  const dump = rolePermissionScreenDump(MODERATOR);

  it("refuses a throwaway copy where one KEY row shows another class's words", () => {
    const swapped = dump
      .split("\n")
      .map((line) => (line.startsWith("row: send a message | KEY")
        ? `row: send a message | KEY | ${RELAY_SENTENCE}`
        : line))
      .join("\n");
    expect(() => checkKeyEnforcementWords(swapped))
      .toThrowError(/KEY row does not show the KEY words: send a message/u);
    expect(() => checkKeyEnforcementWords(swapped))
      .toThrowError(/expected 10 KEY rows showing/u);
  });

  it("refuses a throwaway copy where one RELAY row shows the KEY words", () => {
    const swapped = dump
      .split("\n")
      .map((line) => (line.startsWith("row: join voice | RELAY")
        ? `row: join voice | RELAY | ${KEY_SENTENCE}`
        : line))
      .join("\n");
    expect(() => checkKeyEnforcementWords(swapped))
      .toThrowError(/RELAY row shows the KEY words: join voice/u);
    expect(() => checkKeyEnforcementWords(swapped))
      .toThrowError(/1 RELAY or TRUST rows show the KEY words/u);
  });

  it("refuses a throwaway copy where one TRUST row shows the KEY words", () => {
    const swapped = dump
      .split("\n")
      .map((line) => (line.startsWith("row: ban a member | TRUST")
        ? `row: ban a member | TRUST | ${KEY_SENTENCE}`
        : line))
      .join("\n");
    expect(() => checkKeyEnforcementWords(swapped))
      .toThrowError(/TRUST row shows the KEY words: ban a member/u);
  });

  it("refuses a throwaway copy where one KEY row's words are deleted", () => {
    const without = dump
      .split("\n")
      .map((line) => (line.startsWith("row: read a thread | KEY") ? "row: read a thread | KEY | " : line))
      .join("\n");
    expect(() => checkKeyEnforcementWords(without))
      .toThrowError(/KEY row does not show the KEY words: read a thread/u);
  });
});
