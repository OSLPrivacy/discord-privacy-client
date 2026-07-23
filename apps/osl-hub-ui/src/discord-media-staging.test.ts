import { describe, expect, it } from "vitest";
import {
  DISCORD_MEDIA_CAPTION_MAX_BYTES,
  advanceDiscordMediaStage,
  beginDiscordMediaProtection,
  boundedDiscordMediaCaption,
  cancelDiscordMedia,
  discordMediaPrivatePreviewRequest,
  discordMediaProtectionContract,
  discordMediaTrayView,
  failDiscordMedia,
  parseDiscordMediaSelection,
  removeDiscordMedia,
  reportDiscordMediaProgress,
  retryDiscordMedia,
  sanitizeDiscordMediaFilename,
  setDiscordMediaCaption,
  setDiscordMediaViewOnce,
  stageDiscordMedia,
} from "./discord-media-staging";

const selection = {
  jobId: "R4nd0mOpaqueJobHandle_1234567890",
  metadata: { filename: "photo.png", mediaType: "image/png", size: 2048 },
};

function staged() {
  const item = stageDiscordMedia(selection, 1_000);
  if (!item) throw new Error("fixture did not stage");
  return item;
}

describe("Discord protected media staging", () => {
  it("accepts only an opaque handle and sanitized filename/type/size", () => {
    expect(parseDiscordMediaSelection(selection)).toEqual(selection);
    expect(sanitizeDiscordMediaFilename("C:\\Users\\liam\\ secret\u0000  photo.png ")).toBe("secret photo.png");
    expect(parseDiscordMediaSelection({ ...selection, path: "/secret/photo.png" })).toBeNull();
    expect(parseDiscordMediaSelection({ ...selection, bytes: [1, 2, 3] })).toBeNull();
    expect(parseDiscordMediaSelection({ ...selection, base64: "c2VjcmV0" })).toBeNull();
    expect(parseDiscordMediaSelection({ ...selection, key: "secret" })).toBeNull();
    expect(parseDiscordMediaSelection({ ...selection, metadata: { ...selection.metadata, filename: "../photo.png" } })).toBeNull();
    expect(parseDiscordMediaSelection({ ...selection, metadata: { ...selection.metadata, mediaType: "IMAGE/PNG" } })).toBeNull();
  });

  it("keeps a single staged record and bounds the authenticated caption by UTF-8 bytes", () => {
    let item = staged();
    item = setDiscordMediaCaption(item, "private caption");
    item = setDiscordMediaViewOnce(item, true);
    expect(boundedDiscordMediaCaption("x".repeat(DISCORD_MEDIA_CAPTION_MAX_BYTES))).not.toBeNull();
    expect(boundedDiscordMediaCaption("🙂".repeat(DISCORD_MEDIA_CAPTION_MAX_BYTES))).toBeNull();
    expect(discordMediaProtectionContract(item)).toEqual({
      protocol: "osl-discord-media-v1",
      jobId: selection.jobId,
      metadata: selection.metadata,
      caption: "private caption",
      viewOnce: true,
      authenticatedFields: ["jobId", "metadata", "caption", "viewOnce"],
    });
  });

  it("allows only truthful ordered stages", () => {
    const selected = staged();
    expect(advanceDiscordMediaStage(selected, "uploading", 1_500)).toBe(selected);
    const protecting = beginDiscordMediaProtection(selected, 1_500);
    const uploading = advanceDiscordMediaStage(protecting, "uploading", 2_000);
    const delivering = advanceDiscordMediaStage(uploading, "delivering", 2_500);
    const sent = advanceDiscordMediaStage(delivering, "sent", 3_000);
    expect([protecting.stage, uploading.stage, delivering.stage, sent.stage]).toEqual([
      "protecting", "uploading", "delivering", "sent",
    ]);
    expect(sent.progress).toBe(100);
  });

  it("throttles coarse progress, never regresses, and reserves 100% for delivery", () => {
    const protecting = beginDiscordMediaProtection(staged(), 1_000);
    expect(reportDiscordMediaProgress(protecting, 42, 1_200)).toBe(protecting);
    const at25 = reportDiscordMediaProgress(protecting, 42, 1_500);
    expect(at25.progress).toBe(25);
    expect(reportDiscordMediaProgress(at25, 24, 2_000)).toBe(at25);
    expect(reportDiscordMediaProgress(at25, 100, 2_000)).toBe(at25);
    const uploading = advanceDiscordMediaStage(at25, "uploading", 2_500);
    const delivering = advanceDiscordMediaStage(uploading, "delivering", 3_000);
    expect(reportDiscordMediaProgress(delivering, 100, 3_500).progress).toBe(100);
  });

  it("preserves safe metadata, caption, and view-once across failure, cancellation, and retry", () => {
    let item = setDiscordMediaViewOnce(setDiscordMediaCaption(staged(), "keep me"), true);
    item = advanceDiscordMediaStage(beginDiscordMediaProtection(item, 2_000), "uploading", 2_500);
    const failed = failDiscordMedia(item, "offline");
    expect(failed).toMatchObject({ stage: "failed", retryFrom: "uploading", caption: "keep me", viewOnce: true });
    expect(removeDiscordMedia(item)).toBe(item);
    const retried = retryDiscordMedia(failed, 3_000);
    expect(retried).toMatchObject({ stage: "protecting", caption: "keep me", viewOnce: true });
    const cancelled = cancelDiscordMedia(retried);
    expect(cancelled).toMatchObject({ stage: "cancelled", retryFrom: "protecting", caption: "keep me", viewOnce: true });
    expect(removeDiscordMedia(cancelled)).toBeNull();
  });

  it("exposes private preview only through the opaque native job handle", () => {
    expect(discordMediaPrivatePreviewRequest(staged())).toEqual({ jobId: selection.jobId });
    expect(discordMediaPrivatePreviewRequest(beginDiscordMediaProtection(staged(), 2_000))).toBeNull();
    const item = staged();
    const sent = advanceDiscordMediaStage(
      advanceDiscordMediaStage(advanceDiscordMediaStage(beginDiscordMediaProtection(item, 2_000), "uploading", 2_500), "delivering", 3_000),
      "sent",
      3_500,
    );
    expect(discordMediaPrivatePreviewRequest(sent)).toBeNull();
  });

  it("provides accessible status labels and disables decorative motion when requested", () => {
    const active = beginDiscordMediaProtection(staged(), 2_000);
    expect(discordMediaTrayView(active, true)).toEqual({
      label: "Protected media: photo.png",
      status: "Protecting",
      progressLabel: "Protecting, 0%",
      canEdit: false,
      canCancel: true,
      canRetry: false,
      canRemove: false,
      canPrivatePreview: false,
      motion: "none",
    });
    expect(discordMediaTrayView(failDiscordMedia(active, "offline"), false)).toMatchObject({
      status: "Connection unavailable. Try again when online.", canRetry: true, motion: "coarse",
    });
  });
});
