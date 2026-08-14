import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { oslChatThreadPaneMarkup, type OslChatThreadPaneModel } from "./osl-chat-thread-pane";

describe("TASK 1328 thread pane", () => {
  it("renders one parent message, two thread replies, and a reply box", () => {
    const model: OslChatThreadPaneModel = {
      threadTitle: "Test Thread",
      parentMessage: {
        messageId: "parent-1",
        authorName: "Alice",
        authorId: "alice-123",
        text: "This is the parent message",
        timestamp: Date.now() - 3600000,
      },
      replies: [
        {
          replyId: "reply-1",
          authorName: "Bob",
          authorId: "bob-456",
          text: "First reply",
          timestamp: Date.now() - 1800000,
        },
        {
          replyId: "reply-2",
          authorName: "Charlie",
          authorId: "charlie-789",
          text: "Second reply",
          timestamp: Date.now() - 600000,
        },
      ],
    };

    const markup = oslChatThreadPaneMarkup(model);

    const parentCount = (markup.match(/class="osl-chat-thread-parent"/g) || []).length;
    const replyCount = (markup.match(/class="osl-chat-thread-reply"/g) || []).length;
    const replyBoxCount = (markup.match(/class="osl-chat-thread-reply-box"/g) || []).length;

    console.log(`TASK1328_PARENT_COUNT=${parentCount}`);
    console.log(`TASK1328_REPLY_COUNT=${replyCount}`);
    console.log(`TASK1328_REPLY_BOX=${replyBoxCount}`);

    expect(parentCount).toBe(1);
    expect(replyCount).toBe(2);
    expect(replyBoxCount).toBe(1);

    expect(markup).toContain('data-message-id="parent-1"');
    expect(markup).toContain('data-reply-id="reply-1"');
    expect(markup).toContain('data-reply-id="reply-2"');
    expect(markup).toContain('id="osl-thread-reply-draft"');

    // Write the fixture to file
    const fixtureDir = join(process.cwd(), "screenshots", "artifacts");
    mkdirSync(fixtureDir, { recursive: true });
    const fixturePath = join(fixtureDir, "task-1328-thread-pane-fixture.html");
    const htmlContent = `<!DOCTYPE html>
<html>
<head>
  <meta charset="UTF-8">
  <title>TASK 1328 Thread Pane Fixture</title>
  <style>
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif; padding: 20px; }
    .osl-chat-thread-pane { max-width: 600px; margin: 0 auto; border: 1px solid #ccc; border-radius: 8px; overflow: hidden; }
    .osl-chat-thread-header { background: #f5f5f5; padding: 16px; border-bottom: 1px solid #e0e0e0; }
    .osl-chat-thread-title { margin: 0 0 8px 0; font-size: 18px; font-weight: 600; }
    .osl-chat-thread-parent-info { font-size: 14px; color: #666; }
    .osl-chat-thread-parent { padding: 16px; border-bottom: 1px solid #e0e0e0; background: #fafafa; }
    .osl-chat-parent-header { display: flex; justify-content: space-between; margin-bottom: 8px; font-size: 14px; }
    .osl-chat-parent-author { font-weight: 600; }
    .osl-chat-parent-time { color: #999; }
    .osl-chat-parent-body { font-size: 14px; line-height: 1.5; }
    .osl-chat-thread-replies { border-bottom: 1px solid #e0e0e0; }
    .osl-chat-thread-reply { padding: 12px 16px; border-bottom: 1px solid #f0f0f0; }
    .osl-chat-reply-header { display: flex; justify-content: space-between; margin-bottom: 6px; font-size: 13px; }
    .osl-chat-reply-author { font-weight: 600; }
    .osl-chat-reply-time { color: #999; }
    .osl-chat-reply-body { font-size: 13px; line-height: 1.5; }
    .osl-chat-thread-reply-box { display: flex; flex-direction: column; gap: 8px; padding: 16px; }
    .osl-thread-reply-textarea { padding: 8px; font-family: inherit; font-size: 14px; border: 1px solid #d0d0d0; border-radius: 4px; resize: vertical; min-height: 60px; }
    .osl-thread-reply-send { padding: 8px 16px; background: #0066cc; color: white; border: none; border-radius: 4px; font-weight: 600; cursor: pointer; }
    .osl-thread-reply-send:disabled { background: #ccc; cursor: not-allowed; }
  </style>
</head>
<body>
  ${markup}
</body>
</html>`;
    writeFileSync(fixturePath, htmlContent, "utf-8");
    console.log(`TASK1328_FIXTURE_PATH=${fixturePath}`);
  });

  it("disables the reply send button when the draft is empty or busy", () => {
    const model: OslChatThreadPaneModel = {
      threadTitle: "Test Thread",
      parentMessage: {
        messageId: "parent-1",
        authorName: "Alice",
        authorId: "alice-123",
        text: "This is the parent message",
        timestamp: Date.now(),
      },
      replies: [],
    };

    const markup = oslChatThreadPaneMarkup(model);

    expect(markup).toContain('disabled');
    expect(markup).toContain('class="osl-thread-reply-send"');
  });
});
