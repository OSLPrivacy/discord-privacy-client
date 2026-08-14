import { describe, expect, it, vi } from "vitest";
import {
  previewAutoScrubBadMessageRules,
  renderAutoScrubRulesPage,
  type AutoScrubPreviewConnection,
  type AutoScrubPreviewMessage,
  type AutoScrubRuleSwitch,
} from "./autoscrub-rules-page";

const switches: AutoScrubRuleSwitch[] = [
  { ruleName: "private words", enabled: true, privateWord: "MAPLE-1461" },
];

const messages: AutoScrubPreviewMessage[] = [
  { messageId: "msg-1", channelId: "general", body: "no secrets here" },
  { messageId: "msg-2", channelId: "project-room", body: "the MAPLE-1461 rollout slips a week" },
  { messageId: "msg-3", channelId: "project-room", body: "another mention of maple-1461 in passing" },
];

function connectionOver(list: AutoScrubPreviewMessage[]): AutoScrubPreviewConnection {
  return { readMessages: vi.fn(async () => list) };
}

describe("AutoScrub rules page", () => {
  it("lists every matching item with the exact match count and scanned count", async () => {
    const preview = await previewAutoScrubBadMessageRules(switches, connectionOver(messages));

    expect(preview.scannedMessageCount).toBe(3);
    expect(preview.matchCount).toBe(2);
    expect(preview.matches.map((m) => m.messageId)).toEqual(["msg-2", "msg-3"]);
    expect(preview.deletedCount).toBe(0);

    const markup = renderAutoScrubRulesPage(switches, preview);
    expect(markup).toContain("2 matches out of 3 scanned.");
    expect(markup).toContain("msg-2");
    expect(markup).toContain("msg-3");
    expect(markup).not.toContain("msg-1");
  });

  it("reports zero matches without touching deletedCount when nothing matches", async () => {
    const noMatchMessages = [{ messageId: "msg-9", channelId: "general", body: "irrelevant" }];
    const preview = await previewAutoScrubBadMessageRules(switches, connectionOver(noMatchMessages));

    expect(preview.matchCount).toBe(0);
    expect(preview.deletedCount).toBe(0);
    const markup = renderAutoScrubRulesPage(switches, preview);
    expect(markup).toContain("0 matches out of 1 scanned.");
  });

  it("skips a disabled switch entirely, even if its private word would match", async () => {
    const disabled: AutoScrubRuleSwitch[] = [
      { ruleName: "private words", enabled: false, privateWord: "MAPLE-1461" },
    ];
    const preview = await previewAutoScrubBadMessageRules(disabled, connectionOver(messages));
    expect(preview.matchCount).toBe(0);
  });

  it("skips an enabled switch with a blank private word", async () => {
    const blank: AutoScrubRuleSwitch[] = [{ ruleName: "private words", enabled: true, privateWord: "   " }];
    const preview = await previewAutoScrubBadMessageRules(blank, connectionOver(messages));
    expect(preview.matchCount).toBe(0);
  });

  it("only reads through the connection -- there is no delete method to call", async () => {
    const connection = connectionOver(messages);
    await previewAutoScrubBadMessageRules(switches, connection);
    expect(connection.readMessages).toHaveBeenCalledTimes(1);
    // AutoScrubPreviewConnection has exactly one method; nothing else exists on it.
    expect(Object.keys(connection)).toEqual(["readMessages"]);
  });

  it("renders rule switches and private-word inputs for each rule", () => {
    const markup = renderAutoScrubRulesPage(switches, null);
    expect(markup).toContain('data-autoscrub-rule-switch="private-words"');
    expect(markup).toContain('data-autoscrub-rule-word="private-words"');
    expect(markup).toContain("checked");
    expect(markup).toContain('value="MAPLE-1461"');
    expect(markup).toContain('id="autoscrub-test-rules"');
  });

  it("shows zero deletion buttons on the page, with or without a preview", async () => {
    const preview = await previewAutoScrubBadMessageRules(switches, connectionOver(messages));
    const withPreview = renderAutoScrubRulesPage(switches, preview);
    const withoutPreview = renderAutoScrubRulesPage(switches, null);

    for (const markup of [withPreview, withoutPreview]) {
      const buttons = markup.match(/<button[^>]*>/gi) ?? [];
      // The only button on the page is "Test these rules" -- everything else
      // is a checkbox switch or a text input, neither of which can delete.
      expect(buttons).toEqual(['<button class="button compact" id="autoscrub-test-rules" type="button">']);
      const deleteButtonCount = buttons.filter((tag) => /delete|remove/i.test(tag)).length;
      expect(deleteButtonCount).toBe(0);
      expect(markup).not.toContain("data-autoscrub-delete");
    }
  });
});
