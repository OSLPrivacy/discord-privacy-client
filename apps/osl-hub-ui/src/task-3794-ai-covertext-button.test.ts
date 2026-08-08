import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { pressAiCovertextButton } from "./ai-covertext-button";
import {
  MODEL_PACK_NEEDED,
  NO_CLOUD_AI_USED,
  coverWritingControlsMarkup,
} from "./cover-writing-controls";

const PRIVATE_WORDS = "TASK3794 private words stay exact: maple window 4172.";

describe("TASK3794 AI Covertext button", () => {
  it("writes one readable cover and records no refusal with the pack present", async () => {
    let coverMessages = 0;
    let refusals = 0;
    const opened = new Map<string, string>();
    const outcome = await pressAiCovertextButton({
      modelPackPresent: async () => true,
      writeCoverMessage: async () => {
        coverMessages += 1;
        const cover = `task-3794-local-cover-${coverMessages}`;
        opened.set(cover, PRIVATE_WORDS);
        return cover;
      },
    });
    if (outcome.kind === "refused") refusals += 1;
    expect(outcome.kind).toBe("written");
    const coverMessage = outcome.kind === "written" ? outcome.coverMessage : "";
    expect(opened.get(coverMessage)).toBe(PRIVATE_WORDS);
    expect(coverMessages).toBe(1);
    expect(refusals).toBe(0);
    const button = coverWritingControlsMarkup("discord", { aiAvailable: true });
    expect(button).toContain(NO_CLOUD_AI_USED);
    console.info(`TASK3794 pack=present cover_messages=${coverMessages} readback_exact=${opened.get(coverMessage) === PRIVATE_WORDS} refusals=${refusals} button_words="${NO_CLOUD_AI_USED}"`);
  });

  it("refuses by the exact name and writes nothing with the pack removed", async () => {
    let coverMessages = 0;
    let refusals = 0;
    const outcome = await pressAiCovertextButton({
      modelPackPresent: async () => false,
      writeCoverMessage: async () => {
        coverMessages += 1;
        return "must-not-be-written";
      },
    });
    if (outcome.kind === "refused") refusals += 1;
    const button = coverWritingControlsMarkup("discord", { aiAvailable: false });
    console.info(`TASK3794 pack=removed refusal="${outcome.kind === "refused" ? outcome.reason : "<none>"}" refusals=${refusals} cover_messages=${coverMessages} button_words="${NO_CLOUD_AI_USED}"`);
    expect(outcome, `pack removed produced cover_messages=${coverMessages}`).toEqual({ kind: "refused", reason: MODEL_PACK_NEEDED });
    expect(coverMessages).toBe(0);
    expect(button).toContain(`<small>${MODEL_PACK_NEEDED}</small>`);
    expect(button).toContain(NO_CLOUD_AI_USED);
  });

  it("connects the production button to readiness and the protected cover writer", () => {
    const main = readFileSync(fileURLToPath(new URL("./main.ts", import.meta.url)), "utf8");
    expect(main).toContain('"#native-discord-ai-covertext"');
    expect(main).toContain("void pressNativeDiscordAiCovertext()");
    expect(main).toContain("await loadAiCarrierStatus()");
    expect(main).toContain("await preparePeerProseText(context.contextToken, plaintext");
    expect(main).toContain("void loadAiCarrierStatus().then((status)");
  });
});
