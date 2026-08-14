import { describe, expect, it } from "vitest";
import {
  NEW_FRIEND_DEFAULTS_LEAD,
  NEW_FRIEND_DEFAULTS_TITLE,
  NEW_FRIEND_DEFAULT_GROUPS,
  initialNewFriendDefaults,
  newFriendDefaultsMarkup,
} from "./new-friend-defaults";

/**
 * TASK 0732. The screenshot check in screenshots/new-friend-defaults-capture.test.mjs
 * is the finish line; this is the part of it that does not need a browser --
 * the five named elements, and the wire values that have to keep matching the
 * Rust enums TASK 0249 saves.
 */
describe("new friend defaults screen", () => {
  it("names accounts, conversations, checkmark, Save default and Reset", () => {
    const markup = newFriendDefaultsMarkup(initialNewFriendDefaults());
    for (const name of ["accounts", "conversations", "checkmark", "Save default", "Reset"]) {
      expect(markup).toContain(name);
    }
    expect(markup).toContain('id="save-new-friend-default"');
    expect(markup).toContain('id="reset-new-friend-default"');
    for (const group of NEW_FRIEND_DEFAULT_GROUPS) {
      expect(markup).toContain(`aria-label="${group.name}"`);
    }
  });

  it("says the choices do not reach friends already added", () => {
    expect(NEW_FRIEND_DEFAULTS_TITLE).toBe("New friend defaults");
    expect(NEW_FRIEND_DEFAULTS_LEAD).toContain("from now on");
    expect(NEW_FRIEND_DEFAULTS_LEAD).toContain("already added do not change");
    expect(newFriendDefaultsMarkup(initialNewFriendDefaults())).toContain(NEW_FRIEND_DEFAULTS_LEAD);
  });

  it("offers exactly the choices the backend can parse", () => {
    // These are the strings crates/ipc/src/app_preferences.rs and
    // crates/ipc/src/auto_whitelist_rules.rs accept. A screen that offers a
    // fourth account reach would save something the backend rejects.
    const byName = Object.fromEntries(NEW_FRIEND_DEFAULT_GROUPS.map((group) => [group.name, group.options.map((option) => option.value)]));
    expect(byName.accounts).toEqual(["approved_chats_only", "all_shared_chats"]);
    expect(byName.conversations).toEqual(["never", "ask_me", "always", "only_if_a_friend"]);
    expect(byName.checkmark).toEqual(["always", "never"]);
  });

  it("starts on the same defaults NewFriendDefaults::default() uses", () => {
    expect(initialNewFriendDefaults()).toEqual({
      accountReach: "approved_chats_only",
      conversationRule: "never",
      checkmarkWarning: "always",
    });
  });

  it("marks the screen unsaved only when it is ahead of the saved copy", () => {
    const saved = initialNewFriendDefaults();
    expect(newFriendDefaultsMarkup(saved, saved)).toContain('data-new-friend-status="saved"');
    const edited = { ...saved, conversationRule: "always" as const };
    expect(newFriendDefaultsMarkup(edited, saved)).toContain('data-new-friend-status="unsaved"');
  });

  it("keeps Save default and Reset on the screen in both states", () => {
    const saved = initialNewFriendDefaults();
    for (const markup of [newFriendDefaultsMarkup(saved, saved), newFriendDefaultsMarkup({ ...saved, checkmarkWarning: "never" }, saved)]) {
      expect(markup).toContain(">Save default<");
      expect(markup).toContain(">Reset<");
    }
  });
});
