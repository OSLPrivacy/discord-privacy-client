import { describe, expect, it } from "vitest";
import {
  OslProfilePaneState,
  PROFILE_PANE_FOOTER,
  oslProfilePaneMarkup,
  scopeStorageKey,
  seededProfilePaneRecords,
} from "./osl-profile-pane";

function seededState(): OslProfilePaneState {
  const state = new OslProfilePaneState(seededProfilePaneRecords());
  // TASK 4655 seed: the seed turns on only OSL Chats as a separate profile.
  state.setSeparate("global", false);
  state.setSeparate("osl-chats", true);
  state.setSeparate("enclave:cedar", false);
  state.setSeparate("enclave:maple", false);
  return state;
}

describe("TASK 4655 profile pane", () => {
  it("shows exactly 4 profile rows: OSL profile, OSL Chats, and one per enclave", () => {
    const state = seededState();
    const markup = oslProfilePaneMarkup(state);
    const rowMatches = markup.match(/data-profile-row-scope="[^"]+"/gu) ?? [];
    console.log(`TASK4655 row_count=${rowMatches.length}`);
    console.log(`TASK4655 rows=${rowMatches.join(",")}`);
    expect(rowMatches).toHaveLength(4);
    expect(markup).toContain('data-profile-row-scope="global"');
    expect(markup).toContain('data-profile-row-scope="osl-chats"');
    expect(markup).toContain('data-profile-row-scope="enclave:cedar"');
    expect(markup).toContain('data-profile-row-scope="enclave:maple"');
    expect(markup).toContain(">OSL profile<");
    expect(markup).toContain(">OSL Chats<");
  });

  it("has exactly 1 checked separate-profile checkbox after the seed turns on only OSL Chats", () => {
    const state = seededState();
    const markup = oslProfilePaneMarkup(state);
    const checkedBoxes = markup.match(/data-profile-separate-toggle="[^"]+"\s+checked/gu) ?? [];
    console.log(`TASK4655 checked_count=${checkedBoxes.length}`);
    console.log(`TASK4655 checked=${checkedBoxes.join(",")}`);
    expect(checkedBoxes).toHaveLength(1);
    expect(checkedBoxes[0]).toContain('data-profile-separate-toggle="osl-chats"');
    // Every non-global row carries the exact checkbox copy the task specifies.
    const checkboxLabels = markup.match(/use a separate profile here/gu) ?? [];
    expect(checkboxLabels).toHaveLength(3);
  });

  it("shows all 6 fields for the selected scope", () => {
    const state = seededState();
    state.selectScope("osl-chats");
    const markup = oslProfilePaneMarkup(state);
    const fieldNames = ["display-name", "about-line", "status", "card-background", "avatar", "colour"];
    for (const field of fieldNames) {
      expect(markup).toContain(`data-profile-field="${field}"`);
    }
    const fieldMatches = markup.match(/data-profile-field="[^"]+"/gu) ?? [];
    console.log(`TASK4655 field_count=${fieldMatches.length}`);
    expect(fieldMatches).toHaveLength(6);
  });

  it("uploading avatar file AVATAR-4655 changes only that scope", () => {
    const state = seededState();
    const before = state.resolved();
    const beforeByKey = new Map(before.map((entry) => [scopeStorageKey(entry.scope), entry.profile.avatar]));

    state.uploadAvatar("enclave:maple", "AVATAR-4655");

    const after = state.resolved();
    const afterByKey = new Map(after.map((entry) => [scopeStorageKey(entry.scope), entry.profile.avatar]));
    console.log(`TASK4655 maple_avatar_after=${afterByKey.get("enclave:maple")}`);
    console.log(`TASK4655 unchanged_scopes=${[...afterByKey.entries()].filter(([key, value]) => key !== "enclave:maple" && value === beforeByKey.get(key)).map(([key]) => key).join(",")}`);

    expect(afterByKey.get("enclave:maple")).toBe("AVATAR-4655");
    for (const key of ["global", "osl-chats", "enclave:cedar"]) {
      expect(afterByKey.get(key)).toBe(beforeByKey.get(key));
    }
  });

  it("uploading the OSL Chats avatar does not change the global avatar (TASK 4655b)", () => {
    const state = seededState();
    const globalBefore = state.resolvedFor("global")?.profile.avatar;

    state.uploadAvatar("osl-chats", "AVATAR-4655");

    const globalAfter = state.resolvedFor("global")?.profile.avatar;
    console.log(`TASK4655 global_avatar_before=${globalBefore}`);
    console.log(`TASK4655 global_avatar_after=${globalAfter}`);
    if (globalAfter !== globalBefore) {
      throw new Error(`TASK4655 FAIL global avatar changed: before=${globalBefore} after=${globalAfter}`);
    }
    expect(globalAfter).toBe(globalBefore);
  });

  it("REMOVE returns the avatar to the inherited (global) avatar", () => {
    const state = seededState();
    state.uploadAvatar("enclave:maple", "AVATAR-4655");
    expect(state.resolvedFor("enclave:maple")?.profile.avatar).toBe("AVATAR-4655");

    state.removeAvatar("enclave:maple");

    const resolved = state.resolvedFor("enclave:maple");
    const globalAvatar = state.resolvedFor("global")?.profile.avatar;
    console.log(`TASK4655 maple_avatar_after_remove=${resolved?.profile.avatar}`);
    console.log(`TASK4655 global_avatar=${globalAvatar}`);
    expect(resolved?.profile.avatar).toBe(globalAvatar);
    expect(resolved?.avatarInherited).toBe(true);
    // Removing the avatar does not force the whole scope back to inherited;
    // maple's own useSeparateProfileHere state (off in the seed) is untouched
    // other than the toggle the upload itself turned on.
  });

  it("the footer contains exactly the required sentence", () => {
    const state = seededState();
    const markup = oslProfilePaneMarkup(state);
    expect(PROFILE_PANE_FOOTER).toBe(
      "A profile is a display name and a colour. Your identity is the key, and that does not change here.",
    );
    const footerMatches = markup.match(/<footer class="profile-pane-footer"><p>([^<]*)<\/p><\/footer>/u);
    console.log(`TASK4655 footer=${footerMatches?.[1]}`);
    expect(footerMatches?.[1]).toBe(PROFILE_PANE_FOOTER);
  });
});
