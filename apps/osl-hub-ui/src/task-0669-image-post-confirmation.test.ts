import { describe, expect, it, vi } from "vitest";
import {
  confirmAndSendPreparedImageCopy,
  imagePostConfirmationState,
  IMAGE_POST_AWAITING_CONFIRMATION_REASON,
  type ImagePostConfirmationFixture,
} from "./image-post-confirmation";

const failedQualityFixture: ImagePostConfirmationFixture = {
  originalImageId: "original-0669-private",
  preparedCopy: { imageCopyId: "image-copy-0669prepared-failed" },
  quality: { passed: false, reason: "TASK0669_IMAGE_QUALITY_FAILED" },
  confirmed: true,
};

const passingFixture: ImagePostConfirmationFixture = {
  originalImageId: "original-0669-private",
  preparedCopy: { imageCopyId: "image-copy-0669prepared" },
  quality: { passed: true },
  confirmed: true,
};

describe("TASK 0669 image post confirmation", () => {
  it("reports disabled with its reason on the failed-quality fixture", () => {
    const state = imagePostConfirmationState(failedQualityFixture);
    console.log(`TASK0669_FAILED_STATE=${JSON.stringify(state)}`);

    expect(state.disabled).toBe(true);
    expect(state).toMatchObject({ disabled: true, reason: "TASK0669_IMAGE_QUALITY_FAILED" });
  });

  it("reports the selected copy id matching the prepared copy on the passing fixture", () => {
    const state = imagePostConfirmationState(passingFixture);
    console.log(`TASK0669_PASSING_STATE=${JSON.stringify(state)}`);

    expect(state.disabled).toBe(false);
    expect(state).toMatchObject({ disabled: false, selectedCopyId: passingFixture.preparedCopy.imageCopyId });
  });

  it("never reports the same state for the failed and passing fixtures", () => {
    const failed = imagePostConfirmationState(failedQualityFixture);
    const passing = imagePostConfirmationState(passingFixture);
    console.log(`TASK0669_STATES_DIFFER failed=${JSON.stringify(failed)} passing=${JSON.stringify(passing)}`);

    expect(failed.disabled).not.toBe(passing.disabled);
    expect(JSON.stringify(failed)).not.toBe(JSON.stringify(passing));
  });

  it("keeps post disabled after quality passes until the operator confirms", () => {
    const unconfirmed: ImagePostConfirmationFixture = { ...passingFixture, confirmed: false };
    const state = imagePostConfirmationState(unconfirmed);

    expect(state.disabled).toBe(true);
    expect(state).toMatchObject({ disabled: true, reason: IMAGE_POST_AWAITING_CONFIRMATION_REASON });
  });

  it("sends only the prepared copy id after confirmation, never the private original", async () => {
    const send = vi.fn(async (imageCopyId: string) => imageCopyId);

    const result = await confirmAndSendPreparedImageCopy(passingFixture, send);

    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith(passingFixture.preparedCopy.imageCopyId);
    expect(send).not.toHaveBeenCalledWith(passingFixture.originalImageId);
    expect(result).toBe(passingFixture.preparedCopy.imageCopyId);
  });

  it("refuses to send when the quality check failed, and never calls send", async () => {
    const send = vi.fn(async (imageCopyId: string) => imageCopyId);

    await expect(confirmAndSendPreparedImageCopy(failedQualityFixture, send)).rejects.toThrow(
      "TASK0669_IMAGE_QUALITY_FAILED",
    );
    expect(send).toHaveBeenCalledTimes(0);
  });

  it("refuses to send when quality passed but the operator has not confirmed", async () => {
    const unconfirmed: ImagePostConfirmationFixture = { ...passingFixture, confirmed: false };
    const send = vi.fn(async (imageCopyId: string) => imageCopyId);

    await expect(confirmAndSendPreparedImageCopy(unconfirmed, send)).rejects.toThrow(
      IMAGE_POST_AWAITING_CONFIRMATION_REASON,
    );
    expect(send).toHaveBeenCalledTimes(0);
  });
});
