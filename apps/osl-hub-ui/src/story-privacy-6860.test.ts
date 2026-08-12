import { describe, expect, it } from "vitest";

import manifest from "./story-privacy-6860-manifest.json";
import {
  STORY_AUDIENCES,
  STORY_LIFETIMES,
  STORY_PRIVACY_COPY,
  resolvedComposerAudience,
  shieldRowState,
  storyComposerSendToMarkup,
  storyPrivacySettingsMarkup,
  storyViewerPreOpenMarkup,
  viewerPreOpenCopy,
  type StoryPrivacySettingsState,
} from "./story-privacy-6860";

const supported: StoryPrivacySettingsState = {
  defaultAudience: "story-audience-friends",
  defaultLifetime: "story-lifetime-12h",
  shieldSupported: true,
  shieldOn: true,
  viewReceiptsOn: false,
};

const unsupported: StoryPrivacySettingsState = { ...supported, shieldSupported: false };

describe("story privacy settings", () => {
  it("offers exactly the three audiences and the three auto-burn deadlines", () => {
    expect(STORY_AUDIENCES.map((option) => option.id)).toEqual([
      "story-audience-everyone",
      "story-audience-friends",
      "story-audience-verified",
    ]);
    expect(STORY_LIFETIMES.map((option) => option.label)).toEqual(["1H", "12H", "24H"]);
    expect(STORY_LIFETIMES.map((option) => option.seconds)).toEqual([3600, 43200, 86400]);
  });

  it("marks the stored default as the checked option on both rows", () => {
    const markup = storyPrivacySettingsMarkup(supported);
    expect(markup).toContain('data-story-default-audience="story-audience-friends"');
    expect(markup).toContain(
      '<button class="story-audience-option" type="button" role="radio" data-story-audience="story-audience-friends" aria-checked="true">FRIENDS</button>',
    );
    expect(markup).toContain(
      '<button class="story-lifetime-option" type="button" role="radio" data-story-lifetime="story-lifetime-12h" data-story-lifetime-seconds="43200" aria-checked="true">12H</button>',
    );
    // Exactly one option is checked in each group.
    expect(markup.match(/data-story-audience="[^"]+" aria-checked="true"/g)).toHaveLength(1);
    expect(markup.match(/data-story-lifetime-seconds="\d+" aria-checked="true"/g)).toHaveLength(1);
  });
});

describe("screenshot shield honesty", () => {
  it("claims protection and shows the camera disclosure only where the primitive exists", () => {
    const state = shieldRowState(supported);
    expect(state).toEqual({
      available: true,
      on: true,
      claimsProtection: true,
      copy: STORY_PRIVACY_COPY.shield_disclosure,
    });
    const markup = storyPrivacySettingsMarkup(supported);
    expect(markup).toContain('data-story-shield-available="true"');
    expect(markup).toContain('data-story-shield-claims-protection="true"');
    expect(markup).toContain(STORY_PRIVACY_COPY.shield_disclosure);
    expect(markup).not.toContain("disabled");
  });

  it("disables the control and makes no protection claim where it does not", () => {
    const state = shieldRowState(unsupported);
    expect(state).toEqual({
      available: false,
      on: false,
      claimsProtection: false,
      copy: STORY_PRIVACY_COPY.shield_unavailable,
    });
    const markup = storyPrivacySettingsMarkup(unsupported);
    expect(markup).toContain('data-story-shield-available="false"');
    expect(markup).toContain('data-story-shield-claims-protection="false"');
    expect(markup).toContain('aria-disabled="true"');
    expect(markup).toContain("disabled>OFF</button>");
    expect(markup).toContain(STORY_PRIVACY_COPY.shield_unavailable);
    // The disclosure belongs to a claim; with no claim it must not appear, and
    // neither may any other word that reads as protection.
    expect(markup).not.toContain(STORY_PRIVACY_COPY.shield_disclosure);
    expect(markup).not.toContain("Blocks Windows screen capture");
    expect(markup).not.toMatch(/\bprotected\b/i);
  });

  it("cannot be switched on from a stored value while unsupported", () => {
    const stored: StoryPrivacySettingsState = { ...unsupported, shieldOn: true };
    expect(shieldRowState(stored).on).toBe(false);
    expect(shieldRowState(stored).claimsProtection).toBe(false);
    expect(storyPrivacySettingsMarkup(stored)).toContain(
      'data-story-shield-claims-protection="false"',
    );
  });
});

describe("per-story SEND TO", () => {
  it("seeds from the settings default and reports that it inherited", () => {
    const resolved = resolvedComposerAudience({ settings: supported, sendTo: null });
    expect(resolved).toEqual({ audience: "story-audience-friends", source: "inherited-default" });
    const markup = storyComposerSendToMarkup({ settings: supported, sendTo: null });
    expect(markup).toContain('data-story-audience-source="inherited-default"');
    expect(markup).toContain('data-story-resolved-audience="story-audience-friends"');
    expect(markup).toContain("Burns in 12H");
  });

  it("overrides the default for this story only", () => {
    const resolved = resolvedComposerAudience({
      settings: supported,
      sendTo: "story-audience-verified",
    });
    expect(resolved).toEqual({ audience: "story-audience-verified", source: "send-to-override" });
    const markup = storyComposerSendToMarkup({
      settings: supported,
      sendTo: "story-audience-verified",
    });
    expect(markup).toContain('data-story-audience-source="send-to-override"');
    expect(markup).toContain('data-story-resolved-audience="story-audience-verified"');
    expect(markup).toContain(
      '<button class="story-send-to-option" type="button" role="radio" data-story-send-to="story-audience-verified" aria-checked="true">VERIFIED</button>',
    );
    // The settings default is untouched by a per-story choice.
    expect(supported.defaultAudience).toBe("story-audience-friends");
  });
});

describe("view receipts copy", () => {
  it("tells a viewer a count is kept when receipts are on", () => {
    const state = { ...supported, viewReceiptsOn: true };
    expect(viewerPreOpenCopy(state)).toBe(STORY_PRIVACY_COPY.receipts_on_viewer);
    expect(storyViewerPreOpenMarkup(state)).toContain('data-story-receipts-on="true"');
    expect(storyViewerPreOpenMarkup(state)).toContain(STORY_PRIVACY_COPY.receipts_on_viewer);
  });

  it("tells a viewer nothing is recorded when receipts are off", () => {
    expect(viewerPreOpenCopy(supported)).toBe(STORY_PRIVACY_COPY.receipts_off_viewer);
    const markup = storyViewerPreOpenMarkup(supported);
    expect(markup).toContain('data-story-receipts-on="false"');
    expect(markup).toContain(STORY_PRIVACY_COPY.receipts_off_viewer);
    expect(markup).not.toContain(STORY_PRIVACY_COPY.receipts_on_viewer);
  });
});

describe("the manifest is the single source of the shared strings", () => {
  it("keeps every sentence the Rust engine enforces", () => {
    expect(Object.keys(manifest.copy).sort()).toEqual([
      "receipts_off_viewer",
      "receipts_on_viewer",
      "shield_disclosure",
      "shield_unavailable",
    ]);
    for (const sentence of Object.values(manifest.copy)) {
      expect(sentence.length).toBeGreaterThan(20);
    }
  });
});
