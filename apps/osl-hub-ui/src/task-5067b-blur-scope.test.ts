import { describe, expect, it } from "vitest";
import { loadOslChatThreadBackground, oslChatThreadPaneMarkup, type OslChatThreadPaneModel } from "./osl-chat-thread-pane";

function storage(values: Record<string, string>): Storage {
  return { getItem: (key) => values[key] ?? null } as Storage;
}

const model: OslChatThreadPaneModel = {
  threadTitle: "Proof thread",
  chatId: "chat-a",
  parentMessage: { messageId: "m1", authorName: "Ember", authorId: "person-a", text: "visible message", timestamp: 0 },
  replies: [],
};

function capture(settings: Parameters<typeof oslChatThreadPaneMarkup>[0]["background"]): string {
  return oslChatThreadPaneMarkup({ ...model, background: settings });
}

describe("TASK 5067 blur and chat scope failure proofs", () => {
  it("changes the honest capture when blur toggles and hides a chat-only background in chat B", () => {
    const global = JSON.stringify({ background: "none", blur: false, motion: false, scope: "every-chat" });
    const byChat = JSON.stringify({ "chat-a": { background: "ember", blur: false, motion: false, scope: "chat" } });
    const store = storage({ "osl-chat-background-global-v1": global, "osl-chat-background-by-chat-v1": byChat });
    const a = loadOslChatThreadBackground("chat-a", store);
    const b = loadOslChatThreadBackground("chat-b", store);
    const clearCapture = capture(a);
    const blurredCapture = capture({ ...a, blur: true });
    console.log(`TASK5067_HONEST blur_off_bytes=${clearCapture.length} blur_on_bytes=${blurredCapture.length} different=${clearCapture !== blurredCapture}`);
    console.log(`TASK5067_SCOPE chat_a=${a.background}/${a.scope} chat_b=${b.background}/${b.scope} second_chat_background=${b.background}`);
    expect(clearCapture).not.toBe(blurredCapture);
    expect(b.background).toBe("none");
  });

  it("fails the blur comparison against a throwaway copy whose toggle writes nothing", () => {
    const settings = { background: "ember" as const, blur: false, motion: false, scope: "every-chat" as const };
    const writesNothing = (before: typeof settings, _next: boolean) => before;
    const off = capture(settings);
    const on = capture(writesNothing(settings, true));
    console.log(`TASK5067_MUTANT writes_nothing=true blur_off_bytes=${off.length} blur_on_bytes=${on.length} identical=${off === on}`);
    expect(off).toBe(on);
  });
});
