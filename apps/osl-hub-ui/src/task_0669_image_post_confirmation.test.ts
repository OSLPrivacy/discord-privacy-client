import { describe, expect, it } from "vitest";
import {
  IMAGE_POST_ORIGINAL_SELECTED_REASON,
  IMAGE_POST_QUALITY_FAILED_REASON,
  type ImagePostConfirmationState,
  type ImagePostRequest,
  connectImagePostConfirmation,
  failedQualityImagePostFixture,
  imagePostConfirmationMarkup,
  passingQualityImagePostFixture,
} from "./image-post-confirmation";

describe("TASK 0669 image post confirmation", () => {
  it("failed-quality fixture: direct UI state reports disabled with its reason and confirmation sends nothing", () => {
    const fixture = failedQualityImagePostFixture();
    const posts: ImagePostRequest[] = [];
    const control = connectImagePostConfirmation(fixture, (request) => posts.push(request));

    expect(control.state()).toEqual({ post: "disabled", reason: IMAGE_POST_QUALITY_FAILED_REASON });

    const afterConfirm = control.confirm();
    const afterSecondConfirm = control.confirm();
    expect(afterConfirm).toEqual({ post: "disabled", reason: IMAGE_POST_QUALITY_FAILED_REASON });
    expect(afterSecondConfirm).toEqual(afterConfirm);
    expect(posts).toHaveLength(0);

    const markup = imagePostConfirmationMarkup(control.state());
    expect(markup).toContain('data-image-post="disabled"');
    expect(markup).toMatch(/<button [^>]*\bdisabled\b/u);
    expect(markup).toContain(`data-disabled-reason="${IMAGE_POST_QUALITY_FAILED_REASON}"`);
    expect(markup).not.toContain("data-selected-copy-id");
  });

  it("passing fixture: confirmation sends only the prepared copy and state reports its id", () => {
    const fixture = passingQualityImagePostFixture();
    const posts: ImagePostRequest[] = [];
    const control = connectImagePostConfirmation(fixture, (request) => posts.push(request));

    expect(control.state()).toEqual({ post: "ready-to-confirm" });
    expect(imagePostConfirmationMarkup(control.state())).not.toMatch(/<button [^>]*\bdisabled\b/u);

    const confirmed = control.confirm();
    expect(posts).toHaveLength(1);
    expect(posts[0]).toEqual({ copyId: fixture.preparedCopyId });
    expect(Object.keys(posts[0])).toEqual(["copyId"]);
    expect(JSON.stringify(posts[0])).not.toContain(fixture.originalId);

    expect(confirmed).toEqual({ post: "confirmed", selectedCopyId: fixture.preparedCopyId });
    if (confirmed.post !== "confirmed") throw new Error("unreachable");
    expect(confirmed.selectedCopyId).toBe(fixture.preparedCopyId);
    expect(confirmed.selectedCopyId).not.toBe(fixture.originalId);

    // A second confirmation must not post the copy again.
    expect(control.confirm()).toEqual(confirmed);
    expect(posts).toHaveLength(1);

    const markup = imagePostConfirmationMarkup(control.state());
    expect(markup).toContain('data-image-post="confirmed"');
    expect(markup).toContain(`data-selected-copy-id="${fixture.preparedCopyId}"`);
  });

  it("the two fixtures never report the same state at any step of the flow", () => {
    const failedControl = connectImagePostConfirmation(failedQualityImagePostFixture(), () => {});
    const passingControl = connectImagePostConfirmation(passingQualityImagePostFixture(), () => {});

    const failedStates: ImagePostConfirmationState[] = [failedControl.state()];
    const passingStates: ImagePostConfirmationState[] = [passingControl.state()];
    for (let step = 0; step < 2; step += 1) {
      failedStates.push(failedControl.confirm());
      passingStates.push(passingControl.confirm());
    }

    for (const failed of failedStates) {
      expect(failed.post).toBe("disabled");
      for (const passing of passingStates) {
        expect(passing.post).not.toBe("disabled");
        expect(JSON.stringify(passing)).not.toBe(JSON.stringify(failed));
      }
    }
  });

  it("a disabled state never carries a copy id and a confirmed state never carries a reason", () => {
    const failedControl = connectImagePostConfirmation(failedQualityImagePostFixture(), () => {});
    expect(Object.keys(failedControl.state()).sort()).toEqual(["post", "reason"]);

    const passingControl = connectImagePostConfirmation(passingQualityImagePostFixture(), () => {});
    expect(Object.keys(passingControl.confirm()).sort()).toEqual(["post", "selectedCopyId"]);
  });

  it("refuses to post when the prepared copy is the private original, even with quality passed", () => {
    const fixture = passingQualityImagePostFixture();
    fixture.preparedCopyId = fixture.originalId;
    const posts: ImagePostRequest[] = [];
    const control = connectImagePostConfirmation(fixture, (request) => posts.push(request));

    expect(control.state()).toEqual({ post: "disabled", reason: IMAGE_POST_ORIGINAL_SELECTED_REASON });
    control.confirm();
    expect(posts).toHaveLength(0);
  });
});
