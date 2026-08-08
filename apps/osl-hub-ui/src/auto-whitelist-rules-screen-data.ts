import {
  AUTO_WHITELIST_CHOICES,
  AUTO_WHITELIST_PLACE_KINDS,
  RESET_RULES_EXPLANATION,
  SAVE_RULES_EXPLANATION,
  type AutoWhitelistChoiceId,
} from "./auto-whitelist-rules-screen";

/**
 * Fixed data for the Linux auto-whitelist-rules screenshot (TASK 0740).
 *
 * The saved rules below are chosen so that all four choices appear on the
 * screen at once -- a capture where every row said "never" would prove the
 * rows drew but not that a selected rule shows. They are keyed by place kind
 * rather than by rule key, because the same kind means the same thing in every
 * app: a direct message is a person, a public post is the world.
 */
export const AUTO_WHITELIST_RULES_SCREEN_WINDOW = {
  width: 1920,
  height: 1080,
} as const;

const CHOICE_BY_KIND: Readonly<Record<string, AutoWhitelistChoiceId>> = {
  direct_message: "only_if_a_friend",
  group_direct_message: "only_if_a_friend",
  email_address: "ask_me",
  group_chat: "ask_me",
  group_dm: "ask_me",
  group: "ask_me",
  supergroup: "ask_me",
  server: "ask_me",
  server_channel: "ask_me",
  thread: "ask_me",
  community: "ask_me",
  community_group: "ask_me",
  saved_messages: "always",
  note_to_self: "always",
  channel: "never",
  public_post: "never",
  reply: "never",
  comment: "never",
  story: "never",
  broadcast_list: "never",
  email_domain: "never",
};

/** The rules the screenshot opens with, one per place kind. */
export const AUTO_WHITELIST_RULES_SCREEN_SAVED: Readonly<Record<string, AutoWhitelistChoiceId>> =
  Object.fromEntries(
    AUTO_WHITELIST_PLACE_KINDS.map((place) => {
      const choice = CHOICE_BY_KIND[place.kind];
      if (!choice) throw new Error(`No screenshot rule for place kind: ${place.kind}`);
      return [place.ruleKey, choice];
    }),
  );

/** Text the capture must find on the screen before it counts as drawn. */
export const AUTO_WHITELIST_RULES_SCREEN_REQUIRED_TEXT: readonly string[] = [
  "Auto-whitelist rules",
  `${AUTO_WHITELIST_PLACE_KINDS.length} place kinds, one rule each.`,
  "Save rules",
  "Reset",
  SAVE_RULES_EXPLANATION,
  RESET_RULES_EXPLANATION,
  ...AUTO_WHITELIST_CHOICES.map((choice) => choice.explanation),
  ...[...new Set(AUTO_WHITELIST_PLACE_KINDS.map((place) => place.appLabel))],
  ...[...new Set(AUTO_WHITELIST_PLACE_KINDS.map((place) => place.kindLabel))],
];
