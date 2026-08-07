import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import {
  backendFailures,
  clearBackendFailures,
  recordBackendFailure,
  setBackendFailureConsole,
} from "./backend-failure";

const REFUSAL_KINDS = [
  {
    kind: "text_receive",
    command: "drain_native_discord_overlay_text",
  },
  {
    kind: "attachment_receive",
    command: "list_native_overlay_attachments",
  },
  {
    kind: "visible_row_receive",
    command: "request_native_visible_row_runtime_receipt",
  },
] as const;

const ATTEMPTS_PER_KIND = 10;
const COVER_TEXT = "TASK3988 cover text should not be written";
const SENDER_NAME = "TASK3988 Sender Rowan Vale";
const PRIVATE_WORDS = [
  "TASK3988 private phrase ember lock",
  "TASK3988 private phrase silver gate",
  "TASK3988 private phrase hidden ledger",
] as const;

function walkFiles(root: string): string[] {
  const pending = [root];
  const files: string[] = [];
  while (pending.length > 0) {
    const current = pending.pop();
    if (!current) continue;
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) pending.push(full);
      else if (entry.isFile()) files.push(full);
    }
  }
  return files.sort();
}

function countOccurrences(haystack: string, needle: string): number {
  if (needle.length === 0) return 0;
  let count = 0;
  let index = haystack.indexOf(needle);
  while (index !== -1) {
    count += 1;
    index = haystack.indexOf(needle, index + needle.length);
  }
  return count;
}

function countNeedles(haystack: string, needles: readonly string[]): number {
  return needles.reduce((sum, needle) => sum + countOccurrences(haystack, needle), 0);
}

describe("TASK 3988 refused message privacy audit", () => {
  let auditRoot = "";

  beforeEach(() => {
    clearBackendFailures();
    setBackendFailureConsole(false);
    auditRoot = fs.mkdtempSync(path.join(os.tmpdir(), "osl-task-3988-"));
  });

  afterEach(() => {
    clearBackendFailures();
    setBackendFailureConsole(false);
    if (auditRoot) fs.rmSync(auditRoot, { force: true, recursive: true });
  });

  it("records only refusal counts after receiving repeated refused messages", () => {
    const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    setBackendFailureConsole(true);

    for (const refusal of REFUSAL_KINDS) {
      for (let attempt = 0; attempt < ATTEMPTS_PER_KIND; attempt += 1) {
        recordBackendFailure(
          refusal.command,
          [
            `OSL refused ${refusal.kind}`,
            `cover=${COVER_TEXT}`,
            `sender=${SENDER_NAME}`,
            `private=${PRIVATE_WORDS.join(" | ")}`,
            `attempt=${attempt}`,
          ].join(" "),
          [COVER_TEXT, SENDER_NAME, ...PRIVATE_WORDS],
        );
      }
    }
    setBackendFailureConsole(false);

    const journal = backendFailures();
    const refusalOnlyCounter = journal
      .filter((failure) => failure.kind === "rejected")
      .reduce((sum, failure) => sum + failure.count, 0);
    const kindCounts = new Map(
      REFUSAL_KINDS.map((refusal) => [
        refusal.kind,
        journal
          .filter((failure) => failure.command === refusal.command && failure.kind === "rejected")
          .reduce((sum, failure) => sum + failure.count, 0),
      ]),
    );
    const kindsCovered = [...kindCounts.values()].filter((count) => count === ATTEMPTS_PER_KIND).length;

    const auditFile = path.join(auditRoot, "backend-failures.json");
    fs.writeFileSync(auditFile, JSON.stringify(journal, null, 2));

    const fileText = walkFiles(auditRoot)
      .map((file) => fs.readFileSync(file, "utf8"))
      .join("\n");
    const printedText = consoleSpy.mock.calls.map((call) => String(call[0] ?? "")).join("\n");
    consoleSpy.mockRestore();

    const searchedText = `${fileText}\n${printedText}`;
    const privateWordsFound = countNeedles(searchedText, PRIVATE_WORDS);
    const senderNamesFound = countOccurrences(searchedText, SENDER_NAME);
    const coverTextFound = countOccurrences(searchedText, COVER_TEXT);

    console.log(
      `TASK3988 refusal_kinds_covered=${kindsCovered} kinds=${[...kindCounts.entries()].map(([kind, count]) => `${kind}:${count}`).join(",")}`,
    );
    console.log(
      `TASK3988 searched_files=${walkFiles(auditRoot).length} searched_print_lines=${printedText.split("\n").filter(Boolean).length}`,
    );
    console.log(
      `TASK3988 cover_text_found=${coverTextFound} sender_names_found=${senderNamesFound} private_words_found=${privateWordsFound}`,
    );
    console.log(`TASK3988 refusal_only_counter=${refusalOnlyCounter}`);

    expect(kindsCovered).toBeGreaterThanOrEqual(3);
    expect(refusalOnlyCounter).toBe(REFUSAL_KINDS.length * ATTEMPTS_PER_KIND);
    expect(privateWordsFound).toBe(0);
    expect(senderNamesFound).toBe(0);
    expect(coverTextFound).toBe(0);
    expect(journal.every((failure) => failure.kind === "rejected")).toBe(true);
  });
});
