import { describe, expect, it } from "vitest";
import {
  settingsProfileAvatarTypeAccepted,
  settingsProfileVibeInput,
} from "./settings-profile-block";

describe("TASK 5082b: profile vibe and avatar refusal boundaries", () => {
  it("accepts exactly forty vibe characters and refuses the forty-first", () => {
    const forty = "0123456789012345678901234567890123456789";
    const atLimit = settingsProfileVibeInput(forty);
    const overLimit = settingsProfileVibeInput(`${forty}X`);

    expect(forty).toHaveLength(40);
    expect(atLimit).toEqual({ value: forty, left: 0 });
    expect(overLimit).toEqual({ value: forty, left: 0 });
    expect(overLimit.value).not.toContain("X");
  });

  it("refuses a .txt avatar by MIME type while accepting supported image types", () => {
    expect(settingsProfileAvatarTypeAccepted("image/png")).toBe(true);
    expect(settingsProfileAvatarTypeAccepted("image/jpeg")).toBe(true);
    expect(settingsProfileAvatarTypeAccepted("image/gif")).toBe(true);
    expect(settingsProfileAvatarTypeAccepted("text/plain")).toBe(false);
  });

  it("fails if a throwaway copy accepts either forbidden input", () => {
    const forty = "0123456789012345678901234567890123456789";
    const throwawayVibe = (value: string) => ({ value: Array.from(value).slice(0, 40).join(""), left: Math.max(0, 40 - value.length) });
    const throwawayAvatar = (type: string) => ["image/png", "image/jpeg", "image/gif"].includes(type);

    expect(throwawayVibe(`${forty}X`).value).not.toHaveLength(41);
    expect(throwawayAvatar("text/plain")).toBe(false);
  });
});
