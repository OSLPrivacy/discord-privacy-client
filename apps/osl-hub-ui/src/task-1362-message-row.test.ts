import { describe, expect, it } from "vitest";
import { oslChatMessageRowMarkup, type OslChatMessageRowModel } from "./osl-chat-message-row";
import { writeFileSync, mkdirSync } from "fs";
import { join } from "path";

describe("TASK 1362 reply reference and sender-only edit control", () => {
  it("shows a reply reference on a reply and an edit control only on the sender's own message", () => {
    const otherMessage: OslChatMessageRowModel = {
      messageId: "msg-1",
      authorName: "Alice",
      authorId: "alice-123",
      text: "This is the original message",
      timestamp: Date.now() - 3600000,
      isOwnMessage: false,
    };

    const ownReplyMessage: OslChatMessageRowModel = {
      messageId: "msg-2",
      authorName: "Me",
      authorId: "me-000",
      text: "This is my reply",
      timestamp: Date.now(),
      isOwnMessage: true,
      replyTo: {
        messageId: "msg-1",
        authorName: "Alice",
        text: "This is the original message",
      },
    };

    const otherMarkup = oslChatMessageRowMarkup(otherMessage);
    const ownMarkup = oslChatMessageRowMarkup(ownReplyMessage);
    const combinedMarkup = `${otherMarkup}\n${ownMarkup}`;

    const replyReferenceCount = (combinedMarkup.match(/class="osl-chat-message-reply-reference"/g) || []).length;
    const editControlCount = (combinedMarkup.match(/class="osl-chat-message-edit"/g) || []).length;

    console.log(`TASK1362_REPLY_REFERENCE_COUNT=${replyReferenceCount}`);
    console.log(`TASK1362_EDIT_CONTROL_COUNT=${editControlCount}`);

    // Exactly one reply reference: only the reply message references a parent.
    expect(replyReferenceCount).toBe(1);
    expect(combinedMarkup).toContain('data-reply-to-id="msg-1"');

    // Exactly one edit control: only the sender's own message gets one.
    expect(editControlCount).toBe(1);
    expect(otherMarkup).not.toContain('class="osl-chat-message-edit"');
    expect(ownMarkup).toContain('data-osl-edit-message="msg-2"');

    // Write the fixture to file
    const fixtureDir = join(process.cwd(), "screenshots", "artifacts");
    mkdirSync(fixtureDir, { recursive: true });
    const fixturePath = join(fixtureDir, "task-1362-message-row-fixture.html");
    const htmlContent = `<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>TASK 1362 Message Row Fixture</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; padding: 20px; }
    .osl-chat-message-row { max-width: 600px; margin: 0 auto 12px auto; border: 1px solid #e0e0e0; border-radius: 8px; padding: 12px 16px; background: #fafafa; }
    .osl-chat-message-reply-reference { display: flex; gap: 8px; padding: 6px 10px; margin-bottom: 8px; border-left: 3px solid #0066cc; background: #eef4ff; font-size: 13px; }
    .osl-chat-reply-reference-author { font-weight: 600; }
    .osl-chat-reply-reference-text { color: #555; }
    .osl-chat-message-header { display: flex; justify-content: space-between; margin-bottom: 6px; font-size: 14px; }
    .osl-chat-message-author { font-weight: 600; }
    .osl-chat-message-time { color: #999; }
    .osl-chat-message-body { font-size: 14px; line-height: 1.5; }
    .osl-chat-message-controls { margin-top: 8px; }
    .osl-chat-message-edit { padding: 4px 10px; background: #0066cc; color: white; border: none; border-radius: 4px; font-size: 12px; cursor: pointer; }
  </style>
</head>
<body>
  ${combinedMarkup}
</body>
</html>`;
    writeFileSync(fixturePath, htmlContent, "utf-8");
    console.log(`TASK1362_FIXTURE_PATH=${fixturePath}`);
  });
});
