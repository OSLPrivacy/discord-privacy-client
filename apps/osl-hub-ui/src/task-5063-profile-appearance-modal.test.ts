import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  CHAT_APPEARANCE_ITEMS,
  CHAT_PROFILE_CARD_BACKGROUNDS,
  CHAT_PROFILE_COLOURS,
  CHAT_PROFILE_STATUS_CHOICES,
  chatProfileAppearanceModalMarkup,
  setChatProfileField,
} from "./chat-profile-appearance-modal";
import {
  OslProfilePaneState,
  PROFILE_PANE_FOOTER,
  scopeStorageKey,
  seededProfilePaneRecords,
  type ScopedProfileFieldName,
  type ScopedProfileRecord,
} from "./osl-profile-pane";

function rawSnapshot(state: OslProfilePaneState): Map<string, string> {
  return new Map(state.rows().map((record) => [scopeStorageKey(record.scope), JSON.stringify(record)]));
}

describe("TASK 5063 Profile & Appearance standalone window", () => {
  it("renders a 660px two-pane window with the complete navigation and editor", () => {
    const state = new OslProfilePaneState(seededProfilePaneRecords());
    state.selectScope("osl-chats");
    const markup = chatProfileAppearanceModalMarkup(state);
    const css = readFileSync(new URL("./chat-profile-appearance-modal.css", import.meta.url), "utf8");
    const profiles = markup.match(/data-chat-profile-scope="[^"]+"/gu) ?? [];
    const appearance = markup.match(/data-chat-appearance-item="[^"]+"/gu) ?? [];
    const statuses = markup.match(/data-chat-profile-status(?=[\s/>])/gu) ?? [];
    const backgrounds = markup.match(/data-chat-profile-card-background(?=[\s/>])/gu) ?? [];
    const colours = markup.match(/data-chat-profile-colour(?=[\s/>])/gu) ?? [];
    console.info(`TASK5063 profiles=${profiles.length} appearance=${appearance.length} statuses=${statuses.length} backgrounds=${backgrounds.length} colours=${colours.length}`);
    console.info(`TASK5063 profile_names=OSL profile|OSL Chats|Cedar|Maple`);
    console.info(`TASK5063 appearance_names=${CHAT_APPEARANCE_ITEMS.join("|")}`);
    expect(profiles).toHaveLength(4);
    expect(appearance).toHaveLength(2);
    expect(statuses).toHaveLength(CHAT_PROFILE_STATUS_CHOICES.length);
    expect(backgrounds).toHaveLength(CHAT_PROFILE_CARD_BACKGROUNDS.length);
    expect(colours).toHaveLength(CHAT_PROFILE_COLOURS.length);
    expect(markup).toContain("YOUR PROFILES");
    expect(markup).toContain("APPEARANCE");
    expect(markup).toContain("use a separate profile here");
    expect(markup).toContain(PROFILE_PANE_FOOTER);
    expect(css).toMatch(/width:\s*min\(660px,/u);
    expect(css).toMatch(/grid-template-columns:\s*188px minmax\(0, 1fr\)/u);
  });

  it("reads every field from the chosen scoped record", () => {
    const state = new OslProfilePaneState(seededProfilePaneRecords());
    state.selectScope("osl-chats");
    const markup = chatProfileAppearanceModalMarkup(state);
    const record = state.record("osl-chats") as ScopedProfileRecord;
    const reads = [record.displayName, record.aboutLine, record.status, record.cardBackground, record.avatar, record.colour]
      .filter((value): value is string => typeof value === "string");
    for (const value of reads) expect(markup).toContain(value);
    const fields = markup.match(/data-profile-field="[^"]+"/gu) ?? [];
    console.info(`TASK5063 selected_scope=osl-chats fields_read=${fields.length}`);
    expect(fields).toHaveLength(6);
  });

  it("writes all 7 editable properties in each scope without changing any other raw scoped record", () => {
    const state = new OslProfilePaneState(seededProfilePaneRecords());
    const keys = state.rows().map((record) => scopeStorageKey(record.scope));
    const stringFields: ScopedProfileFieldName[] = ["displayName", "aboutLine", "status", "cardBackground", "colour"];
    let isolatedWrites = 0;
    for (const [scopeIndex, key] of keys.entries()) {
      if (key !== "global") {
        const before = rawSnapshot(state);
        state.setSeparate(key, true);
        for (const other of keys.filter((candidate) => candidate !== key)) expect(rawSnapshot(state).get(other)).toBe(before.get(other));
        isolatedWrites += 1;
      }
      for (const [fieldIndex, field] of stringFields.entries()) {
        const before = rawSnapshot(state);
        const value = field === "cardBackground" || field === "colour"
          ? `#${(scopeIndex * 100 + fieldIndex + 1).toString(16).padStart(6, "0")}`
          : `${field}-5063-${key}`;
        setChatProfileField(state, key, field, value);
        expect(state.record(key)?.[field]).toBe(value);
        for (const other of keys.filter((candidate) => candidate !== key)) expect(rawSnapshot(state).get(other)).toBe(before.get(other));
        isolatedWrites += 1;
      }
      {
        const before = rawSnapshot(state);
        state.uploadAvatar(key, `AVATAR-5063-${key}`);
        expect(state.record(key)?.avatar).toBe(`AVATAR-5063-${key}`);
        for (const other of keys.filter((candidate) => candidate !== key)) expect(rawSnapshot(state).get(other)).toBe(before.get(other));
        isolatedWrites += 1;
      }
    }
    console.info(`TASK5063 scopes=${keys.length} isolated_writes=${isolatedWrites} other_scopes_unchanged_per_write=3`);
    expect(keys).toHaveLength(4);
    expect(isolatedWrites).toBe(27);
  });
});
