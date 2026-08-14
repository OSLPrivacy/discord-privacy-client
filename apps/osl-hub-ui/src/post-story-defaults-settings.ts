/**
 * TASK 4656 — Settings defaults for post visibility, story visibility and
 * story lifetime.
 *
 * Stable IDs mirror the shipping engine exactly (`crates/content-defaults`):
 * the four visibility options are frozen by TASK 4651
 * (`vis.everyone` / `vis.chosen` / `vis.except` / `vis.onlyme`), and the four
 * story-lifetime options are frozen by this task's `do:` line
 * (`life.1h` / `life.24h` / `life.72h` / `life.7d`).
 *
 * Posts keep the default-only model: there is no per-post visibility
 * control anywhere in this file or its markup, only the two Settings
 * defaults below (`postVisibility`, `storyVisibility`) plus the lifetime
 * default. A story may still be given a per-story SEND TO override, but
 * that control belongs to the story composer (TASK 6858), not to Settings —
 * this module only reads and writes the three defaults.
 */

export type VisibilityOptionId = "vis.everyone" | "vis.chosen" | "vis.except" | "vis.onlyme";
export type StoryLifetimeId = "life.1h" | "life.24h" | "life.72h" | "life.7d";

export interface VisibilityOptionDescription {
  readonly id: VisibilityOptionId;
  readonly label: string;
}

export interface StoryLifetimeDescription {
  readonly id: StoryLifetimeId;
  readonly label: string;
}

export const visibilityOptions: readonly VisibilityOptionDescription[] = [
  { id: "vis.everyone", label: "Everyone" },
  { id: "vis.chosen", label: "Chosen people" },
  { id: "vis.except", label: "Everyone except" },
  { id: "vis.onlyme", label: "Only me" },
];

export const storyLifetimeOptions: readonly StoryLifetimeDescription[] = [
  { id: "life.1h", label: "1 hour" },
  { id: "life.24h", label: "24 hours" },
  { id: "life.72h", label: "72 hours" },
  { id: "life.7d", label: "7 days" },
];

const visibilityOptionIds = new Set(visibilityOptions.map((option) => option.id));
const storyLifetimeIds = new Set(storyLifetimeOptions.map((option) => option.id));

export function isVisibilityOptionId(value: string | null): value is VisibilityOptionId {
  return value !== null && visibilityOptionIds.has(value as VisibilityOptionId);
}

export function isStoryLifetimeId(value: string | null): value is StoryLifetimeId {
  return value !== null && storyLifetimeIds.has(value as StoryLifetimeId);
}

export const DEFAULT_POST_VISIBILITY: VisibilityOptionId = "vis.everyone";
export const DEFAULT_STORY_VISIBILITY: VisibilityOptionId = "vis.everyone";
export const DEFAULT_STORY_LIFETIME: StoryLifetimeId = "life.24h";

export const postVisibilityStorageKey = "osl-hub-post-visibility-default";
export const storyVisibilityStorageKey = "osl-hub-story-visibility-default";
export const storyLifetimeStorageKey = "osl-hub-story-lifetime-default";

type DefaultsStorage = Pick<Storage, "getItem" | "setItem">;

export interface PostStoryDefaults {
  readonly postVisibility: VisibilityOptionId;
  readonly storyVisibility: VisibilityOptionId;
  readonly storyLifetime: StoryLifetimeId;
}

/**
 * Reads the three defaults from persistent storage, seeding any that have
 * never been set. Called again after a restart, this returns exactly what
 * the previous session last saved — `storage` is the only source of truth,
 * there is no separate in-memory default that could drift from it.
 */
export function initializePostStoryDefaults(storage: DefaultsStorage): PostStoryDefaults {
  const rawPostVisibility = storage.getItem(postVisibilityStorageKey);
  const postVisibility = isVisibilityOptionId(rawPostVisibility) ? rawPostVisibility : DEFAULT_POST_VISIBILITY;
  if (!isVisibilityOptionId(rawPostVisibility)) storage.setItem(postVisibilityStorageKey, postVisibility);

  const rawStoryVisibility = storage.getItem(storyVisibilityStorageKey);
  const storyVisibility = isVisibilityOptionId(rawStoryVisibility) ? rawStoryVisibility : DEFAULT_STORY_VISIBILITY;
  if (!isVisibilityOptionId(rawStoryVisibility)) storage.setItem(storyVisibilityStorageKey, storyVisibility);

  const rawStoryLifetime = storage.getItem(storyLifetimeStorageKey);
  const storyLifetime = isStoryLifetimeId(rawStoryLifetime) ? rawStoryLifetime : DEFAULT_STORY_LIFETIME;
  if (!isStoryLifetimeId(rawStoryLifetime)) storage.setItem(storyLifetimeStorageKey, storyLifetime);

  return { postVisibility, storyVisibility, storyLifetime };
}

export function savePostVisibilityDefault(storage: DefaultsStorage, id: string): VisibilityOptionId {
  if (!isVisibilityOptionId(id)) throw new Error(`OSL: unknown post visibility default ${JSON.stringify(id)}`);
  storage.setItem(postVisibilityStorageKey, id);
  return id;
}

export function saveStoryVisibilityDefault(storage: DefaultsStorage, id: string): VisibilityOptionId {
  if (!isVisibilityOptionId(id)) throw new Error(`OSL: unknown story visibility default ${JSON.stringify(id)}`);
  storage.setItem(storyVisibilityStorageKey, id);
  return id;
}

export function saveStoryLifetimeDefault(storage: DefaultsStorage, id: string): StoryLifetimeId {
  if (!isStoryLifetimeId(id)) throw new Error(`OSL: unknown story lifetime default ${JSON.stringify(id)}`);
  storage.setItem(storyLifetimeStorageKey, id);
  return id;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function visibilityChoiceGroup(
  groupName: "post" | "story",
  label: string,
  explanation: string,
  selected: VisibilityOptionId,
): string {
  const buttons = visibilityOptions.map((option) => (
    `<button class="post-story-default-choice ${selected === option.id ? "selected" : ""}" type="button" role="radio" aria-checked="${selected === option.id}" data-post-story-default-group="${groupName}-visibility" data-post-story-default-choice="${option.id}">${escapeHtml(option.label)}</button>`
  )).join("");
  return `<article class="post-story-default-row" data-post-story-default-kind="${groupName}-visibility"><div><strong>${escapeHtml(label)}</strong><small>${escapeHtml(explanation)}</small></div><div class="post-story-default-choices" role="radiogroup" aria-label="${escapeHtml(label)}">${buttons}</div></article>`;
}

/**
 * The three Settings controls this task adds. Posts get exactly one
 * control here (the default) and nothing else — there is no per-post
 * variant of this markup anywhere in the UI.
 */
export function postStoryDefaultsSettingsMarkup(defaults: PostStoryDefaults): string {
  const postGroup = visibilityChoiceGroup(
    "post",
    "Post visibility",
    "Who receives a key to your posts. Posts always use this default; there is no per-post choice.",
    defaults.postVisibility,
  );
  const storyGroup = visibilityChoiceGroup(
    "story",
    "Story visibility",
    "Who receives a key to your stories by default. A story can override this with its own SEND TO choice.",
    defaults.storyVisibility,
  );
  const lifetimeButtons = storyLifetimeOptions.map((option) => (
    `<button class="post-story-default-choice ${defaults.storyLifetime === option.id ? "selected" : ""}" type="button" role="radio" aria-checked="${defaults.storyLifetime === option.id}" data-post-story-default-group="story-lifetime" data-post-story-default-choice="${option.id}">${escapeHtml(option.label)}</button>`
  )).join("");
  const lifetimeGroup = `<article class="post-story-default-row" data-post-story-default-kind="story-lifetime"><div><strong>Story lifetime</strong><small>How long a story stays available before it expires.</small></div><div class="post-story-default-choices" role="radiogroup" aria-label="Story lifetime">${lifetimeButtons}</div></article>`;

  return `<details class="settings-disclosure post-story-defaults-settings" open><summary>Posts and stories</summary><div class="post-story-defaults-list">${postGroup}${storyGroup}${lifetimeGroup}</div></details>`;
}
