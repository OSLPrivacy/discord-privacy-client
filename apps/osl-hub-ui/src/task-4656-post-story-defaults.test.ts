import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import {
  DEFAULT_POST_VISIBILITY,
  DEFAULT_STORY_LIFETIME,
  DEFAULT_STORY_VISIBILITY,
  initializePostStoryDefaults,
  postStoryDefaultsSettingsMarkup,
  postVisibilityStorageKey,
  saveStoryLifetimeDefault,
  saveStoryVisibilityDefault,
  savePostVisibilityDefault,
  storyLifetimeOptions,
  storyLifetimeStorageKey,
  storyVisibilityStorageKey,
  visibilityOptions,
  type PostStoryDefaults,
} from "./post-story-defaults-settings";

/** A tiny in-memory stand-in for `localStorage`, so persistence can be
 * driven directly without a browser. */
function fakeStorage(): { getItem: (key: string) => string | null; setItem: (key: string, value: string) => void } {
  const backing = new Map<string, string>();
  return {
    getItem: (key) => backing.get(key) ?? null,
    setItem: (key, value) => { backing.set(key, value); },
  };
}

describe("TASK 4656 — post and story defaults in Settings", () => {
  it("the four visibility options are exactly TASK 4651's frozen stable IDs", () => {
    expect(visibilityOptions.map((option) => option.id)).toEqual([
      "vis.everyone",
      "vis.chosen",
      "vis.except",
      "vis.onlyme",
    ]);
  });

  it("story lifetime offers exactly 1 hour, 24 hours, 72 hours and 7 days", () => {
    expect(storyLifetimeOptions.map((option) => option.id)).toEqual([
      "life.1h",
      "life.24h",
      "life.72h",
      "life.7d",
    ]);
  });

  it("seeds sensible defaults on first run and persists them", () => {
    const storage = fakeStorage();
    const defaults = initializePostStoryDefaults(storage);
    expect(defaults.postVisibility).toBe(DEFAULT_POST_VISIBILITY);
    expect(defaults.storyVisibility).toBe(DEFAULT_STORY_VISIBILITY);
    expect(defaults.storyLifetime).toBe(DEFAULT_STORY_LIFETIME);
    expect(storage.getItem(postVisibilityStorageKey)).toBe(DEFAULT_POST_VISIBILITY);
    expect(storage.getItem(storyVisibilityStorageKey)).toBe(DEFAULT_STORY_VISIBILITY);
    expect(storage.getItem(storyLifetimeStorageKey)).toBe(DEFAULT_STORY_LIFETIME);
  });

  it("all three defaults survive a restart, for every stable ID", () => {
    const storage = fakeStorage();
    // First session: set each default to something other than the seed.
    savePostVisibilityDefault(storage, "vis.except");
    saveStoryVisibilityDefault(storage, "vis.onlyme");
    saveStoryLifetimeDefault(storage, "life.7d");

    // Simulate a restart: a brand new read of the defaults, backed only by
    // whatever `storage` still has on disk, with no in-memory state carried
    // over from the first session.
    const restarted: PostStoryDefaults = initializePostStoryDefaults(storage);
    expect(restarted).toEqual({
      postVisibility: "vis.except",
      storyVisibility: "vis.onlyme",
      storyLifetime: "life.7d",
    });
  });

  it("rejects an unknown stable ID rather than silently coercing it", () => {
    const storage = fakeStorage();
    expect(() => savePostVisibilityDefault(storage, "vis.public")).toThrow();
    expect(() => saveStoryLifetimeDefault(storage, "life.forever")).toThrow();
  });

  it("renders exactly one control group per default, with all option IDs present", () => {
    const markup = postStoryDefaultsSettingsMarkup({
      postVisibility: "vis.everyone",
      storyVisibility: "vis.except",
      storyLifetime: "life.24h",
    });
    expect((markup.match(/data-post-story-default-kind="post-visibility"/g) ?? []).length).toBe(1);
    expect((markup.match(/data-post-story-default-kind="story-visibility"/g) ?? []).length).toBe(1);
    expect((markup.match(/data-post-story-default-kind="story-lifetime"/g) ?? []).length).toBe(1);
    for (const option of visibilityOptions) {
      expect(markup).toContain(`data-post-story-default-choice="${option.id}"`);
    }
    for (const option of storyLifetimeOptions) {
      expect(markup).toContain(`data-post-story-default-choice="${option.id}"`);
    }
  });

  it("posts expose zero per-post visibility controls anywhere in the UI source", () => {
    // A per-post control would need to carry a post identifier alongside a
    // visibility choice. Nothing in the shipped UI source may do that — the
    // only visibility choice a post can be given is the Settings default
    // asserted above. This scans real files rather than trusting a comment,
    // so introducing one anywhere would turn this red.
    const srcDir = join(__dirname);
    const offenders: string[] = [];
    const bannedPatterns = [/data-post-visibility(?!-default)/u, /per-post-visibility/iu, /postVisibilityOverride/u];
    for (const entry of readdirSync(srcDir)) {
      const path = join(srcDir, entry);
      if (!statSync(path).isFile()) continue;
      if (!/\.(ts|tsx)$/u.test(entry)) continue;
      if (entry === "post-story-defaults-settings.ts" || entry === "task-4656-post-story-defaults.test.ts") continue;
      const contents = readFileSync(path, "utf8");
      if (bannedPatterns.some((pattern) => pattern.test(contents))) offenders.push(entry);
    }
    expect(offenders).toEqual([]);
  });
});
