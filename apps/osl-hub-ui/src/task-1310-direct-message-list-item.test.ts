import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  OSL_CHANGED_BUILD_CHAT_WARNING,
  oslChatDirectMessageBuildState,
  oslChatDirectMessageListItemMarkup,
  oslChatDirectMessageListMarkup,
  type OslChatDirectMessageBuildWarning,
  type OslChatDirectMessageListItemModel,
} from "./osl-chat-direct-message-list-item";

const artifactDir = path.resolve(fileURLToPath(new URL("../screenshots/artifacts", import.meta.url)));

const CHANGED_BUILD_WARNING: OslChatDirectMessageBuildWarning = {
  kind: "changedBuild",
  reason: "changed",
  message: OSL_CHANGED_BUILD_CHAT_WARNING,
  messageSendingAvailable: true,
};

/**
 * The fixture list. Exactly ONE direct-message item, and that one item carries
 * exactly ONE changed-build warning state -- the finish line for TASK 1310.
 */
const FIXTURE_ITEM: OslChatDirectMessageListItemModel = {
  conversationId: "dm-ava-1310",
  name: "Ava Lindqvist",
  latest: {
    body: "Sending you the cabin photos tonight.",
    direction: "incoming",
    timestampLabel: "09:14",
  },
  unreadCount: 2,
  active: true,
  buildWarning: CHANGED_BUILD_WARNING,
};

function countOf(markup: string, pattern: RegExp): number {
  return markup.match(pattern)?.length ?? 0;
}

describe("TASK 1310 direct-message list item", () => {
  it("renders a fixture list with one direct-message item and one changed-build warning state", () => {
    const markup = oslChatDirectMessageListMarkup({ items: [FIXTURE_ITEM] });

    const itemCount = countOf(markup, /data-osl-chat-dm-kind="direct-message"/gu);
    const warningCount = countOf(markup, /data-osl-chat-dm-build-warning="[a-z-]+"/gu);
    const buildState = markup.match(/data-osl-chat-dm-build="([a-z-]+)"/u)?.[1] ?? "none";
    const name = markup.match(/class="osl-chat-dm-name">([^<]*)</u)?.[1] ?? "";
    const latest = markup.match(/class="osl-chat-dm-latest-speaker">([^<]*):<\/span> ([^<]*)</u) ?? [];
    const warningText = markup.match(/<small>([^<]*)<\/small>/u)?.[1] ?? "";
    const sendingAvailable = markup.match(/data-message-sending-available="([a-z]+)"/u)?.[1] ?? "none";
    const openDisabled = countOf(markup, /class="osl-chat-dm-open"[^>]*disabled/gu);

    const fixturePath = path.join(artifactDir, "task-1310-direct-message-list-fixture.html");
    fs.mkdirSync(artifactDir, { recursive: true });
    fs.writeFileSync(fixturePath, fixtureDocument(markup), "utf8");

    console.log(`TASK1310_DIRECT_MESSAGE_ITEM_COUNT=${itemCount}`);
    console.log(`TASK1310_ITEM_NAME=${name}`);
    console.log(`TASK1310_ITEM_LATEST_SPEAKER=${latest[1] ?? ""}`);
    console.log(`TASK1310_ITEM_LATEST_MESSAGE=${latest[2] ?? ""}`);
    console.log(`TASK1310_CHANGED_BUILD_WARNING_COUNT=${warningCount}`);
    console.log(`TASK1310_BUILD_STATE=${buildState}`);
    console.log(`TASK1310_WARNING_TEXT=${warningText}`);
    console.log(`TASK1310_MESSAGE_SENDING_AVAILABLE=${sendingAvailable}`);
    console.log(`TASK1310_OPEN_DISABLED_COUNT=${openDisabled}`);
    console.log(`TASK1310_FIXTURE_PATH=${fixturePath}`);

    expect(itemCount).toBe(1);
    expect(name).toBe("Ava Lindqvist");
    expect(latest[1]).toBe("Ava Lindqvist");
    expect(latest[2]).toBe("Sending you the cabin photos tonight.");
    expect(warningCount).toBe(1);
    expect(buildState).toBe("changed");
    expect(warningText).toBe(OSL_CHANGED_BUILD_CHAT_WARNING);
    // A changed build warns; it never removes chat features (installed_build.rs).
    expect(sendingAvailable).toBe("true");
    expect(openDisabled).toBe(0);
    expect(fs.existsSync(fixturePath)).toBe(true);
  });

  it("shows no warning at all for an unmodified build", () => {
    const markup = oslChatDirectMessageListMarkup({
      items: [{ ...FIXTURE_ITEM, buildWarning: null }],
    });
    const warningCount = countOf(markup, /data-osl-chat-dm-build-warning=/gu);
    const buildState = markup.match(/data-osl-chat-dm-build="([a-z-]+)"/u)?.[1] ?? "none";
    console.log(`TASK1310_UNMODIFIED_WARNING_COUNT=${warningCount}`);
    console.log(`TASK1310_UNMODIFIED_BUILD_STATE=${buildState}`);
    expect(warningCount).toBe(0);
    expect(buildState).toBe("unmodified");
    // No reassuring badge either: absence of a warning is not proof of anything.
    expect(markup).not.toContain("Verified build");
  });

  it("distinguishes a corrupt startup proof from a changed build", () => {
    const markup = oslChatDirectMessageListItemMarkup({
      ...FIXTURE_ITEM,
      buildWarning: { ...CHANGED_BUILD_WARNING, reason: "corruptProof" },
    });
    const buildState = markup.match(/data-osl-chat-dm-build="([a-z-]+)"/u)?.[1] ?? "none";
    console.log(`TASK1310_CORRUPT_PROOF_BUILD_STATE=${buildState}`);
    expect(buildState).toBe("corrupt-proof");
    expect(oslChatDirectMessageBuildState(undefined)).toBe("unmodified");
  });

  it("escapes the name and the latest message", () => {
    const markup = oslChatDirectMessageListItemMarkup({
      conversationId: "dm-<script>",
      name: '<img src=x onerror="alert(1)">',
      latest: { body: "<b>bold</b>", direction: "outgoing", timestampLabel: "10:00" },
    });
    expect(markup).not.toContain("<img src=x");
    expect(markup).not.toContain("<b>bold</b>");
    expect(markup).toContain("&lt;b&gt;bold&lt;/b&gt;");
  });

  it("keeps the row when previews are hidden and when there is no message", () => {
    const hidden = oslChatDirectMessageListItemMarkup({ ...FIXTURE_ITEM, previewVisible: false });
    expect(hidden).toContain("Preview hidden");
    expect(hidden).not.toContain("cabin photos");
    const empty = oslChatDirectMessageListItemMarkup({ conversationId: "dm-ben", name: "Ben", latest: null });
    expect(empty).toContain("No messages yet");
  });
});

function fixtureDocument(list: string): string {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="color-scheme" content="dark light" />
    <title>OSL Privacy - TASK 1310 direct message list</title>
    <link rel="stylesheet" href="../../src/styles.css" />
    <style>
      body { margin: 0; display: grid; place-items: center; min-height: 100vh; }
      .task-1310-frame { width: 1280px; height: 800px; padding: 24px; box-sizing: border-box; display: grid; place-items: start center; }
      .task-1310-panel { width: 288px; border: 1px solid var(--line); background: var(--bg); }
      .task-1310-panel > header { min-height: 56px; padding: 0 14px; border-bottom: 1px solid var(--line); display: flex; align-items: center; }
      .task-1310-panel > header h1 { margin: 0; font-size: 13px; font-weight: 600; letter-spacing: .02em; }
    </style>
  </head>
  <body>
    <div class="task-1310-frame">
      <div class="task-1310-panel">
        <header><h1>Direct messages</h1></header>
        ${list}
      </div>
    </div>
  </body>
</html>
`;
}
