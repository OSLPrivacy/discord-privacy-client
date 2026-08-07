import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  createRefusedMessagePrivacyAudit,
  receiveRefusedMessage,
  refusedMessageKinds,
  searchRefusedMessagePrivacyAudit,
} from "./refused-message-privacy";

const COVER_TEXT = "COVER-TASK3988-MAPLE";
const SENDER_NAME = "Sender Task3988 Maple";
const PRIVATE_WORDS = "private task 3988 words";

describe("TASK 3988 refused message privacy audit", () => {
  it("records only refusal counts after receiving repeated refused messages", () => {
    const root = path.join(os.tmpdir(), `osl-task-3988-${process.pid}-${Date.now()}`);
    const audit = createRefusedMessagePrivacyAudit(root);

    for (const kind of refusedMessageKinds) {
      for (let index = 0; index < 10; index += 1) {
        receiveRefusedMessage(audit, {
          kind,
          coverText: COVER_TEXT,
          senderName: SENDER_NAME,
          privateWords: PRIVATE_WORDS,
        });
      }
    }

    const summary = refusedMessageKinds
      .map((kind) => `${kind}:${audit.counts[kind]}`)
      .join(",");
    const search = searchRefusedMessagePrivacyAudit(audit, {
      coverText: COVER_TEXT,
      senderName: SENDER_NAME,
      privateWords: PRIVATE_WORDS,
    });
    const coveredKinds = refusedMessageKinds.filter((kind) => audit.counts[kind] > 0).length;

    console.log(`TASK3988 refusal_kinds_covered=${coveredKinds} kinds=${summary}`);
    console.log(
      `TASK3988 searched_files=${search.searchedFiles} searched_print_lines=${search.searchedPrintLines}`,
    );
    console.log(
      `TASK3988 cover_text_found=${search.coverTextFound} sender_names_found=${search.senderNamesFound} private_words_found=${search.privateWordsFound}`,
    );
    console.log(`TASK3988 refusal_only_counter=${search.refusalOnlyCounter}`);

    expect(coveredKinds).toBeGreaterThanOrEqual(3);
    expect(audit.counts.text_receive).toBe(10);
    expect(audit.counts.attachment_receive).toBe(10);
    expect(audit.counts.visible_row_receive).toBe(10);
    expect(search.searchedFiles).toBeGreaterThan(0);
    expect(search.searchedPrintLines).toBeGreaterThan(0);
    if (search.coverTextFound !== 0) {
      throw new Error(`TASK3988 cover_text_found=${search.coverTextFound} cover_text=${COVER_TEXT}`);
    }
    if (search.senderNamesFound !== 0) {
      throw new Error(`TASK3988 sender_names_found=${search.senderNamesFound} sender_name=${SENDER_NAME}`);
    }
    if (search.privateWordsFound !== 0) {
      throw new Error(`TASK3988 private_words_found=${search.privateWordsFound} private_words=${PRIVATE_WORDS}`);
    }
    expect(search.refusalOnlyCounter).toBeGreaterThan(0);
  });
});
