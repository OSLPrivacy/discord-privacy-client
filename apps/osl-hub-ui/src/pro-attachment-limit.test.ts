import { describe, expect, it } from "vitest";

import { PRO_ATTACHMENT_LIMIT_BYTES, proAttachmentTooLargeScreen } from "./pro-attachment-limit";

const PRO_FILE_1_1_GB_BYTES = Math.round(PRO_ATTACHMENT_LIMIT_BYTES * 1.1);
const EXACT_MESSAGE = "This file is 1.1 GB (1,181,116,006 bytes). Pro files are limited to 1 GB (1,073,741,824 bytes).";

describe("TASK0049 Pro attachment too-large screen", () => {
  it("shows the exact selected size and 1 GB Pro limit without an upgrade action", () => {
    const screen = proAttachmentTooLargeScreen(PRO_FILE_1_1_GB_BYTES);
    const upgradeActions = screen.actions.filter((action) => /upgrade|pro|plan/i.test(`${action.id} ${action.label}`));

    expect(screen).toMatchObject({
      title: "File is too large",
      fileSizeBytes: 1_181_116_006,
      limitBytes: 1_073_741_824,
      message: EXACT_MESSAGE,
    });
    expect(screen.actions).toEqual([{ id: "choose-smaller-file", label: "Choose a smaller file" }]);
    expect(upgradeActions).toHaveLength(0);
    console.log(`TASK0049 message="${screen.message}" file_size_bytes=${screen.fileSizeBytes} limit_bytes=${screen.limitBytes} upgrade_action_count=${upgradeActions.length}`);
  });

  it("refuses to render the over-limit screen for a file inside the Pro limit", () => {
    expect(() => proAttachmentTooLargeScreen(PRO_ATTACHMENT_LIMIT_BYTES)).toThrow("above the Pro limit");
  });
});
