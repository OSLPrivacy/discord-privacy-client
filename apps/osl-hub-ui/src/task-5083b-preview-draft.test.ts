import { describe, expect, it } from "vitest";
import { settingsProfileCardMarkup } from "./settings-profile-block";
import { OslProfilePaneState, seededProfilePaneRecords, type ScopedProfileRecord } from "./osl-profile-pane";

describe("TASK 5083b: preview cannot show the past", () => {
  it("shows an unsaved draft name, while a saved-record copy demonstrably stays old", () => {
    const state = new OslProfilePaneState(seededProfilePaneRecords());
    const savedRecord = state.record("global") as ScopedProfileRecord;
    const savedCopy = { ...savedRecord, scope: { kind: "global" as const } };
    state.setField("global", "displayName", "Preview Draft 5083b");
    const draftCard = settingsProfileCardMarkup(state.record("global") as ScopedProfileRecord);
    const savedRecordCard = settingsProfileCardMarkup(savedCopy);

    console.info(`TASK5083B draft_card_name=Preview Draft 5083b saved_record_card_name=${savedCopy.displayName}`);
    expect(draftCard).toContain("Preview Draft 5083b");
    expect(draftCard).not.toContain(savedCopy.displayName);
    expect(savedRecordCard).toContain(savedCopy.displayName);
    expect(savedRecordCard).not.toContain("Preview Draft 5083b");
  });
});
