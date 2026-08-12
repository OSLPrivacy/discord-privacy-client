/**
 * TASK 6860 — render the shipped story privacy surface for the check.
 *
 * The Python check grades the real markup this module produces rather than a
 * description of it, so gutting the surface shows up as a missing string.
 */

import { writeFileSync } from "node:fs";

import {
  STORY_AUDIENCES,
  STORY_LIFETIMES,
  STORY_PRIVACY_COPY,
  resolvedComposerAudience,
  shieldRowState,
  storyComposerSendToMarkup,
  storyPrivacySettingsMarkup,
  storyViewerPreOpenMarkup,
  type StoryPrivacySettingsState,
} from "../src/story-privacy-6860";

const base: StoryPrivacySettingsState = {
  defaultAudience: "story-audience-friends",
  defaultLifetime: "story-lifetime-12h",
  shieldSupported: true,
  shieldOn: true,
  viewReceiptsOn: false,
};

const unsupported: StoryPrivacySettingsState = {
  ...base,
  shieldSupported: false,
  shieldOn: true,
};

const receiptsOn: StoryPrivacySettingsState = { ...base, viewReceiptsOn: true };

const out = process.argv[2];
if (!out) {
  throw new Error("TASK6860 render: an output path is required");
}

writeFileSync(
  out,
  JSON.stringify(
    {
      task: "6860",
      audiences: STORY_AUDIENCES,
      lifetimes: STORY_LIFETIMES,
      copy: STORY_PRIVACY_COPY,
      supported: {
        shield: shieldRowState(base),
        settings_markup: storyPrivacySettingsMarkup(base),
      },
      unsupported: {
        shield: shieldRowState(unsupported),
        settings_markup: storyPrivacySettingsMarkup(unsupported),
      },
      receipts_on: {
        settings_markup: storyPrivacySettingsMarkup(receiptsOn),
        viewer_markup: storyViewerPreOpenMarkup(receiptsOn),
      },
      receipts_off: {
        viewer_markup: storyViewerPreOpenMarkup(base),
      },
      composer_inherited: {
        resolved: resolvedComposerAudience({ settings: base, sendTo: null }),
        markup: storyComposerSendToMarkup({ settings: base, sendTo: null }),
      },
      composer_override: {
        resolved: resolvedComposerAudience({ settings: base, sendTo: "story-audience-verified" }),
        markup: storyComposerSendToMarkup({ settings: base, sendTo: "story-audience-verified" }),
      },
    },
    null,
    2,
  ),
  "utf8",
);
console.log(`TASK6860 rendered story privacy surface to ${out}`);
