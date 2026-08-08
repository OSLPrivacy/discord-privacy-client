import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  chooseMessageDefault,
  FACTORY_MESSAGE_DEFAULTS,
  initialMessageDefaultsScreenState,
  MESSAGE_DEFAULTS_EXPLANATIONS,
  messageDefaultsScreenMarkup,
  messageDefaultsUnsaved,
  messageDefaultsWirePayload,
  resetMessageDefaults,
  savedMessageDefaultLabels,
  saveMessageDefaults,
  type MessageDefaults,
} from "./message-defaults";

const SAVED: MessageDefaults = {
  burnScope: "app",
  timerSeconds: 86_400,
  viewOnceLengthSeconds: 45,
  coverWriting: "ai_covertext",
};

function rustSource(relative: string): string {
  return readFileSync(fileURLToPath(new URL(`../../../${relative}`, import.meta.url)), "utf8");
}

describe("message defaults screen", () => {
  it("shows the title, four headings and one explanation each", () => {
    const markup = messageDefaultsScreenMarkup(initialMessageDefaultsScreenState(SAVED));
    expect(markup).toContain('<h1 id="route-heading" tabindex="-1">Message defaults</h1>');
    for (const control of ["timer", "burn-scope", "view-once-length", "writing"] as const) {
      const { legend, why } = MESSAGE_DEFAULTS_EXPLANATIONS[control];
      expect(markup).toContain(`<legend>${legend}</legend>`);
      expect(markup).toContain(`data-message-default-why="${control}">${why}`);
      expect(why.trim().split(/\s+/u).length).toBeGreaterThanOrEqual(12);
    }
  });

  it("marks the four saved values and nothing else as saved", () => {
    const markup = messageDefaultsScreenMarkup(initialMessageDefaultsScreenState(SAVED));
    expect(savedMessageDefaultLabels(SAVED)).toEqual({
      timer: "1 day",
      "burn-scope": "This app",
      "view-once-length": "45 seconds",
      writing: "AI",
    });
    expect(markup.match(/msg-def-saved-tag/gu)).toHaveLength(4);
    for (const value of ["86400", "app", "45", "ai_covertext"]) {
      expect(markup).toContain(`value="${value}" data-message-default=`);
    }
    expect(markup).toContain('data-message-default-status="saved"');
  });

  it("carries a save and a reset control", () => {
    const markup = messageDefaultsScreenMarkup(initialMessageDefaultsScreenState(SAVED));
    expect(markup).toContain('id="save-message-defaults" type="button" data-message-default-save>Save<');
    expect(markup).toContain('id="reset-message-defaults" type="button" data-message-default-reset>Reset<');
  });

  it("changes one control at a time and only saves on Save", () => {
    let state = initialMessageDefaultsScreenState(SAVED);
    expect(messageDefaultsUnsaved(state)).toBe(false);
    state = chooseMessageDefault(state, "timer", "3600");
    expect(state.draft.timerSeconds).toBe(3_600);
    expect(state.saved.timerSeconds).toBe(86_400);
    expect(messageDefaultsUnsaved(state)).toBe(true);
    expect(messageDefaultsScreenMarkup(state)).toContain('data-message-default-status="unsaved"');
    state = saveMessageDefaults(state);
    expect(state.saved.timerSeconds).toBe(3_600);
    expect(messageDefaultsUnsaved(state)).toBe(false);
  });

  it("refuses a value that is not one of the offered choices", () => {
    const state = initialMessageDefaultsScreenState(SAVED);
    expect(chooseMessageDefault(state, "timer", "7")).toBe(state);
    expect(chooseMessageDefault(state, "burn-scope", "everything")).toBe(state);
    expect(chooseMessageDefault(state, "view-once-length", "999")).toBe(state);
    expect(chooseMessageDefault(state, "writing", "human")).toBe(state);
  });

  it("resets the form to the factory values without saving them", () => {
    const state = resetMessageDefaults(initialMessageDefaultsScreenState(SAVED));
    expect(state.draft).toEqual(FACTORY_MESSAGE_DEFAULTS);
    expect(state.saved).toEqual(SAVED);
    expect(messageDefaultsUnsaved(state)).toBe(true);
  });

  it("sends the four values in the shape cmd_osl_save_message_defaults accepts", () => {
    expect(messageDefaultsWirePayload(SAVED)).toEqual({
      burn_scope: "app",
      timer_seconds: 86_400,
      view_once_length_seconds: 45,
      cover_writing: "ai_covertext",
    });
    const dto = rustSource("crates/ipc/src/commands.rs");
    const struct = dto.slice(dto.indexOf("pub struct MessageDefaultsDto"));
    for (const field of Object.keys(messageDefaultsWirePayload(SAVED))) {
      expect(struct.slice(0, struct.indexOf("}"))).toContain(`pub ${field}:`);
    }
  });

  it("keeps the factory values equal to the Rust defaults", () => {
    const prefs = rustSource("crates/ipc/src/app_preferences.rs");
    const timer = /fn default_message_timer_seconds\(\) -> u32 \{\s*([0-9_]+)/u.exec(prefs);
    const viewOnce = /fn default_display_length_seconds\(\) -> u32 \{\s*([0-9_]+)/u.exec(prefs);
    expect(Number(timer?.[1].replace(/_/gu, ""))).toBe(FACTORY_MESSAGE_DEFAULTS.timerSeconds);
    expect(Number(viewOnce?.[1].replace(/_/gu, ""))).toBe(FACTORY_MESSAGE_DEFAULTS.viewOnceLengthSeconds);
    const scope = prefs.slice(prefs.indexOf("pub enum MessageScopeDefault"));
    expect(scope.slice(0, scope.indexOf("}"))).toContain("#[default]\n    Message,");
    const writer = prefs.slice(prefs.indexOf("pub enum MessageWriterDefault"));
    expect(writer.slice(0, writer.indexOf("}"))).toContain("#[default]\n    Plaintext,");
    expect(FACTORY_MESSAGE_DEFAULTS.burnScope).toBe("message");
    expect(FACTORY_MESSAGE_DEFAULTS.coverWriting).toBe("plaintext");
  });
});
