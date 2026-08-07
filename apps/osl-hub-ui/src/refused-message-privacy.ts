import fs from "node:fs";
import path from "node:path";

export const refusedMessageKinds = [
  "text_receive",
  "attachment_receive",
  "visible_row_receive",
] as const;

export type RefusedMessageKind = typeof refusedMessageKinds[number];

export interface RefusedMessageInput {
  kind: RefusedMessageKind;
  coverText: string;
  senderName: string;
  privateWords: string;
}

export interface RefusedMessagePrivacyAudit {
  root: string;
  printLines: string[];
  receiptPath: string;
  counts: Record<RefusedMessageKind, number>;
}

export interface RefusedMessagePrivacySearch {
  searchedFiles: number;
  searchedPrintLines: number;
  coverTextFound: number;
  senderNamesFound: number;
  privateWordsFound: number;
  refusalOnlyCounter: number;
}

export function createRefusedMessagePrivacyAudit(root: string): RefusedMessagePrivacyAudit {
  fs.mkdirSync(root, { recursive: true });
  const audit: RefusedMessagePrivacyAudit = {
    root,
    printLines: [],
    receiptPath: path.join(root, "refused-message-counts.json"),
    counts: {
      text_receive: 0,
      attachment_receive: 0,
      visible_row_receive: 0,
    },
  };
  writeRefusalCounts(audit);
  return audit;
}

export function receiveRefusedMessage(
  audit: RefusedMessagePrivacyAudit,
  message: RefusedMessageInput,
): void {
  audit.counts[message.kind] += 1;
  writeRefusalCounts(audit);
  audit.printLines.push(
    `OSL refused inbound message kind=${message.kind} count=${audit.counts[message.kind]}`,
  );
}

export function searchRefusedMessagePrivacyAudit(
  audit: RefusedMessagePrivacyAudit,
  forbidden: Pick<RefusedMessageInput, "coverText" | "senderName" | "privateWords">,
): RefusedMessagePrivacySearch {
  const diskText = fs
    .readdirSync(audit.root)
    .map((entry) => fs.readFileSync(path.join(audit.root, entry), "utf8"))
    .join("\n");
  const printText = audit.printLines.join("\n");
  const haystack = `${diskText}\n${printText}`;
  return {
    searchedFiles: fs.readdirSync(audit.root).length,
    searchedPrintLines: audit.printLines.length,
    coverTextFound: countOccurrences(haystack, forbidden.coverText),
    senderNamesFound: countOccurrences(haystack, forbidden.senderName),
    privateWordsFound: countOccurrences(haystack, forbidden.privateWords),
    refusalOnlyCounter: Object.values(audit.counts).reduce((sum, count) => sum + count, 0),
  };
}

function writeRefusalCounts(audit: RefusedMessagePrivacyAudit): void {
  fs.writeFileSync(
    audit.receiptPath,
    `${JSON.stringify({ refusedMessages: audit.counts }, null, 2)}\n`,
    "utf8",
  );
}

function countOccurrences(haystack: string, needle: string): number {
  if (needle.length === 0) return 0;
  let count = 0;
  let offset = 0;
  while (true) {
    const found = haystack.indexOf(needle, offset);
    if (found === -1) return count;
    count += 1;
    offset = found + needle.length;
  }
}
