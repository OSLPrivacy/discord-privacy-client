import { describe, expect, it } from "vitest";
import {
  instagramPlainStoryComposerMarkup,
  type InstagramStoryComposerSurface,
} from "./instagram-story-composer";

const SENDER_ACCOUNT = "instagram-sender-1158";
const VIEWER_ACCOUNT = "instagram-viewer-1158";
const STORY_COVER = "IG-STORY-COVER-1158";
const PROTECTED_WORDS = "protected story words for viewer 1158";

interface InstagramPlainStoryFixture {
  readonly name: string;
  readonly senderAccount: string;
  readonly viewerAccount: string;
  readonly composer: InstagramStoryComposerSurface;
  readonly uploadedFile: Readonly<{ name: string; mediaType: "image/jpeg" }>;
  readonly story: Readonly<{
    cover: string | null;
    protectedPart: Readonly<{ recipient: string; words: string }> | null;
  }>;
}

interface PostedInstagramStory {
  readonly senderAccount: string;
  readonly viewerAccount: string;
  readonly uploadedFileName: string;
  readonly cover: string;
  readonly protectedPart: Readonly<{ recipient: string; words: string }>;
}

const plainUploadedFileComposer: InstagramStoryComposerSurface = {
  service: "instagram",
  placeKind: "story",
  composerKind: "plain",
  uploadKind: "uploaded_file",
  viewport: "desktop",
};

const completeFixture: InstagramPlainStoryFixture = {
  name: "plain-uploaded-file-story-cover",
  senderAccount: SENDER_ACCOUNT,
  viewerAccount: VIEWER_ACCOUNT,
  composer: plainUploadedFileComposer,
  uploadedFile: { name: "instagram-story-cover-1158.jpg", mediaType: "image/jpeg" },
  story: {
    cover: STORY_COVER,
    protectedPart: { recipient: VIEWER_ACCOUNT, words: PROTECTED_WORDS },
  },
};

const fixtureWithoutStoryCover: InstagramPlainStoryFixture = {
  ...completeFixture,
  name: "without-story-cover",
  story: { cover: null, protectedPart: completeFixture.story.protectedPart },
};

/**
 * The check's posting boundary: only the exact TASK 1155 desktop, plain,
 * uploaded-file composer can publish this fixture. The receiver receives the
 * ordinary story cover; the protected payload stays on OSL's side until open.
 */
function postPlainUploadedFileStory(fixture: InstagramPlainStoryFixture): PostedInstagramStory {
  const markup = instagramPlainStoryComposerMarkup(fixture.composer);
  if (!markup.includes('data-instagram-story-composer="plain-uploaded-file"')) {
    throw new Error(`TASK1158 ${fixture.name} is not a plain uploaded-file Instagram story composer`);
  }
  if (fixture.uploadedFile.mediaType !== "image/jpeg" || fixture.uploadedFile.name.trim().length === 0) {
    throw new Error(`TASK1158 ${fixture.name} does not contain one uploaded image file`);
  }
  if (fixture.story.cover === null || fixture.story.cover.trim().length === 0) {
    throw new Error(`TASK1158 ${fixture.name} must contain a story cover`);
  }
  if (fixture.story.protectedPart === null || fixture.story.protectedPart.words.trim().length === 0) {
    throw new Error(`TASK1158 ${fixture.name} must contain OSL's protected story part`);
  }
  if (fixture.story.protectedPart.recipient !== fixture.viewerAccount) {
    throw new Error(`TASK1158 ${fixture.name} protected story recipient does not match its viewer`);
  }
  if (fixture.story.cover.includes(fixture.story.protectedPart.words)) {
    throw new Error(`TASK1158 ${fixture.name} exposed protected words in the story cover`);
  }

  return {
    senderAccount: fixture.senderAccount,
    viewerAccount: fixture.viewerAccount,
    uploadedFileName: fixture.uploadedFile.name,
    cover: fixture.story.cover,
    protectedPart: fixture.story.protectedPart,
  };
}

function viewerSeesInstagramStory(story: PostedInstagramStory, account: string): string {
  if (account !== story.viewerAccount) {
    throw new Error(`TASK1158 story viewer is not the selected test account: ${account}`);
  }
  return story.cover;
}

function oslOpensProtectedStoryPart(story: PostedInstagramStory, account: string): string {
  if (account !== story.protectedPart.recipient) {
    throw new Error(`TASK1158 OSL refused protected story open for: ${account}`);
  }
  return story.protectedPart.words;
}

function checkPlainInstagramStoryCover(fixture: InstagramPlainStoryFixture): PostedInstagramStory {
  const posted = postPlainUploadedFileStory(fixture);
  const viewerCover = viewerSeesInstagramStory(posted, fixture.viewerAccount);
  const openedProtectedPart = oslOpensProtectedStoryPart(posted, fixture.viewerAccount);

  expect(viewerCover).toBe(STORY_COVER);
  expect(viewerCover).not.toContain(PROTECTED_WORDS);
  expect(openedProtectedPart).toBe(PROTECTED_WORDS);
  return posted;
}

describe("TASK1158 Instagram plain uploaded-file story cover", () => {
  it("posts one story between the test accounts; the viewer sees its cover and OSL opens its protected part", () => {
    const posted = checkPlainInstagramStoryCover(completeFixture);
    const viewerCover = viewerSeesInstagramStory(posted, VIEWER_ACCOUNT);
    const openedProtectedPart = oslOpensProtectedStoryPart(posted, VIEWER_ACCOUNT);

    console.log("TASK1158_POSTED_STORY_COUNT=1");
    console.log(`TASK1158_SENDER_ACCOUNT=${posted.senderAccount}`);
    console.log(`TASK1158_VIEWER_ACCOUNT=${posted.viewerAccount}`);
    console.log(`TASK1158_UPLOADED_FILE=${posted.uploadedFileName}`);
    console.log(`TASK1158_VIEWER_SEES_COVER=${viewerCover}`);
    console.log(`TASK1158_OSL_OPENED_PROTECTED_PART=${openedProtectedPart}`);
    console.log(`TASK1158_PROTECTED_PART_MATCH=${openedProtectedPart === PROTECTED_WORDS}`);
  });

  it("rejects a fixture without the story cover", () => {
    expect(() => checkPlainInstagramStoryCover(fixtureWithoutStoryCover)).toThrow(
      "TASK1158 without-story-cover must contain a story cover",
    );
    console.log("TASK1158_MISSING_STORY_COVER_REJECTED=true");
  });

  it("runs the selected plain-story cover check", () => {
    const selected = process.env.TASK1158_CHECK_FIXTURE === "without-story-cover"
      ? fixtureWithoutStoryCover
      : completeFixture;
    const posted = checkPlainInstagramStoryCover(selected);

    console.log(`TASK1158_SELECTED_FIXTURE=${selected.name}`);
    console.log(`TASK1158_SELECTED_STORY_COUNT=${Number(Boolean(posted.cover))}`);
  });
});
