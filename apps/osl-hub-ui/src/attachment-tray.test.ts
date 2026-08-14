import { describe, expect, it } from "vitest";

import {
  addAttachmentToTray,
  attachmentTrayMarkup,
  EMPTY_ATTACHMENT_TRAY_STATE,
  removeAttachmentFromTray,
} from "./attachment-tray";
import { DISCORD_MEDIA_MAX_SIZE } from "./discord-media-staging";

const PHOTO = { jobId: "AbCdEfGhIjKlMnOpQrStUv", metadata: { filename: "photo.png", mediaType: "image/png", size: 512 } };
const CLIP = { jobId: "ZyXwVuTsRqPoNmLkJiHgFe", metadata: { filename: "clip.mp4", mediaType: "video/mp4", size: 4_096 } };
const OVERSIZE_MOVIE = {
  jobId: "QqRrSsTtUuVvWwXxYyZzAa",
  metadata: { filename: "movie.mov", mediaType: "video/quicktime", size: DISCORD_MEDIA_MAX_SIZE + 1 },
};

describe("TASK 1333 multi-file attachment tray", () => {
  it("keeps two valid cards after one oversize file is refused", () => {
    const now = 1_000;
    const afterPhoto = addAttachmentToTray(EMPTY_ATTACHMENT_TRAY_STATE, PHOTO, now);
    const afterClip = addAttachmentToTray(afterPhoto, CLIP, now);
    const afterOversize = addAttachmentToTray(afterClip, OVERSIZE_MOVIE, now);

    expect(afterOversize.cards).toHaveLength(2);
    expect(afterOversize.cards.map((card) => card.metadata.filename)).toEqual(["photo.png", "clip.mp4"]);
    expect(afterOversize.lastRefusal).toEqual({ filename: "movie.mov", reason: "oversize" });

    const markup = attachmentTrayMarkup(afterOversize);
    expect(markup).toContain("photo.png");
    expect(markup).toContain("clip.mp4");
    expect(markup.match(/data-attachment-job=/gu)).toHaveLength(2);
    expect(markup).toContain("movie.mov was not added because it is too large.");

    console.log(
      `TASK1333 cards=${afterOversize.cards.length} names=${afterOversize.cards.map((card) => card.metadata.filename).join(",")} `
      + `refused_filename=${afterOversize.lastRefusal?.filename} refused_reason=${afterOversize.lastRefusal?.reason}`,
    );
  });

  it("keeps existing cards and reports invalid, not oversize, for a malformed selection", () => {
    const now = 1_000;
    const afterPhoto = addAttachmentToTray(EMPTY_ATTACHMENT_TRAY_STATE, PHOTO, now);
    const afterMalformed = addAttachmentToTray(afterPhoto, "not-a-selection", now);

    expect(afterMalformed.cards).toHaveLength(1);
    expect(afterMalformed.lastRefusal).toEqual({ filename: null, reason: "invalid" });
  });

  it("removes a card by job id without disturbing the others", () => {
    const now = 1_000;
    const afterPhoto = addAttachmentToTray(EMPTY_ATTACHMENT_TRAY_STATE, PHOTO, now);
    const afterClip = addAttachmentToTray(afterPhoto, CLIP, now);

    const afterRemove = removeAttachmentFromTray(afterClip, PHOTO.jobId);
    expect(afterRemove.cards).toHaveLength(1);
    expect(afterRemove.cards[0]?.jobId).toBe(CLIP.jobId);
  });
});
