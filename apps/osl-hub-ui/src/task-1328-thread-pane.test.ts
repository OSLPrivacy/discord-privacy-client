import { describe, expect, it } from "vitest";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  oslChatThreadPaneMarkup,
  type OslChatThreadPaneModel,
} from "./osl-chat-thread-pane";

function model(overrides: Partial<OslChatThreadPaneModel> = {}): OslChatThreadPaneModel {
  return {
    parent: {
      messageId: "parent-1",
      author: "Rose",
      body: "What time works for everyone?",
      timestampLabel: "10:00",
    },
    replies: [
      {
        replyId: "reply-1",
        author: "You",
        body: "How about 2pm?",
        timestampLabel: "10:05",
      },
      {
        replyId: "reply-2",
        author: "Sam",
        body: "2pm works for me.",
        timestampLabel: "10:07",
      },
    ],
    replyDraft: "",
    busy: false,
    ...overrides,
  };
}

function screenshotHtml(markup: string): string {
  const styles = readFileSync(new URL("./styles.css", import.meta.url), "utf8");
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"/><title>TASK 1328 Thread pane</title><style>${styles.replaceAll("</style", "<\\/style")}</style><style>html,body,#app{width:100%;height:100%;margin:0;background:#080c0d;overflow:hidden}.app-frame{width:100%;height:100%}</style></head><body><div id="app">${markup}</div></body></html>`;
}

const FIXTURE_DIR = path.resolve("screenshots/artifacts");
const FIXTURE_PATH = path.join(FIXTURE_DIR, "task-1328-thread-pane-fixture.html");

describe("TASK 1328 thread pane", () => {
  it("renders one parent message, two thread replies, and a reply box", () => {
    const markup = oslChatThreadPaneMarkup(model());

    const parentMatches = [...markup.matchAll(/class="osl-chat-thread-parent"/gu)];
    const replyMatches = [...markup.matchAll(/class="osl-chat-thread-reply"/gu)];

    expect(parentMatches.length).toBe(1);
    expect(replyMatches.length).toBe(2);
    expect(markup).toContain('data-message-id="parent-1"');
    expect(markup).toContain('data-reply-id="reply-1"');
    expect(markup).toContain('data-reply-id="reply-2"');
    expect(markup).toContain('data-osl-thread-reply="parent-1"');
    expect(markup).toContain('id="osl-thread-reply-draft"');
    expect(markup).toContain("What time works for everyone?");
    expect(markup).toContain("How about 2pm?");
    expect(markup).toContain("2pm works for me.");

    mkdirSync(FIXTURE_DIR, { recursive: true });
    writeFileSync(FIXTURE_PATH, screenshotHtml(markup));

    console.log(`TASK1328_PARENT_COUNT=${parentMatches.length}`);
    console.log(`TASK1328_REPLY_COUNT=${replyMatches.length}`);
    console.log(`TASK1328_REPLY_BOX=1`);
    console.log(`TASK1328_FIXTURE_PATH=${path.relative(process.cwd(), FIXTURE_PATH)}`);
  });

  it("disables the reply send button when the draft is empty or busy", () => {
    const empty = oslChatThreadPaneMarkup(model());
    expect(empty).toMatch(/class="osl-chat-thread-reply-send" type="submit"[^>]* disabled/u);

    const busy = oslChatThreadPaneMarkup(model({ replyDraft: "Sounds good", busy: true }));
    expect(busy).toMatch(/class="osl-chat-thread-reply-send" type="submit"[^>]* disabled/u);

    const ready = oslChatThreadPaneMarkup(model({ replyDraft: "Sounds good" }));
    expect(ready).toMatch(/class="osl-chat-thread-reply-send" type="submit"(?![^>]* disabled)[^>]*>/u);
  });
});
