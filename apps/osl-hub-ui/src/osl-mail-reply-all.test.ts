import { describe, expect, it } from "vitest";
import { planOslMailReplyAll } from "./osl-mail-reply-all";

describe("OSL Mail Reply All", () => {
  it("never joins BCC recipients to Reply All", () => {
    const source = {
      to: ["lee@example.com"],
      cc: ["sam@example.com"],
      bcc: ["jo@example.com"],
    };
    const beforeCount = 0;

    const withoutBcc = planOslMailReplyAll(source, { includeBcc: false });
    const withBcc = planOslMailReplyAll(source, { includeBcc: true });

    console.info(`TASK 1293 before protected-recipient count=${beforeCount}`);
    console.info(`TASK 1293 include BCC no result names=${withoutBcc.protectedRecipients.join(" and ")} count=${withoutBcc.protectedRecipientCount}`);
    console.info(`TASK 1293 include BCC yes refused=${withBcc.refusal}`);
    console.info(`TASK 1293 include BCC yes result names=${withBcc.protectedRecipients.join(" and ")} count=${withBcc.protectedRecipientCount}`);

    expect(beforeCount).toBe(0);
    expect(withoutBcc.protectedRecipientCount).toBe(2);
    expect(withoutBcc.protectedRecipients).toEqual(["lee@example.com", "sam@example.com"]);
    expect(withoutBcc.protectedRecipients).not.toContain("jo@example.com");
    expect(withBcc.refusal).toBe("BCC excluded from Reply All");
    expect(withBcc.protectedRecipients).toEqual(["lee@example.com", "sam@example.com"]);
    expect(withBcc.protectedRecipients).not.toContain("jo@example.com");
    expect(withBcc.protectedRecipientCount).toBe(2);
  });
});
