import { describe, expect, it } from "vitest";
import { appearanceLivePreviewMarkup, SAFETY_NUMBER_AUTHENTICATION_COPY } from "./appearance-live-preview";
import { appearanceSettingsContent } from "./appearance-settings-section";
import { defaultAppearancePreferences } from "./appearance-preferences";
import type { ScopedProfileRecord } from "./osl-profile-pane";

const draft: ScopedProfileRecord = {
  scope: { kind: "global" }, useSeparateProfileHere: false, displayName: "Avery", aboutLine: "",
  status: "Making tea", cardBackground: "#101214", avatar: null, colour: "#2ac0f0",
};

function proofCounts(markup: string): Record<string, number> {
  return {
    Verified: (markup.match(/Verified/gu) ?? []).length,
    "verified-via": (markup.match(/verified-via/gu) ?? []).length,
    connectedServiceProof: (markup.match(/connected-service(?:-proof)?/gu) ?? []).length,
  };
}

function isHonestPreview(markup: string): boolean {
  return Object.values(proofCounts(markup)).every((count) => count === 0)
    && (markup.match(new RegExp(SAFETY_NUMBER_AUTHENTICATION_COPY, "gu")) ?? []).length === 1;
}

describe("TASK 5083 honest live Appearance preview", () => {
  it("changes each draft field immediately without saving and renders no identity proof", () => {
    const initial = appearanceLivePreviewMarkup(draft, "#2ac0f0");
    const accentChanged = appearanceLivePreviewMarkup(draft, "#ff977f");
    const nameChanged = appearanceLivePreviewMarkup({ ...draft, displayName: "Mina" }, "#2ac0f0");
    const vibeChanged = appearanceLivePreviewMarkup({ ...draft, status: "On a walk" }, "#2ac0f0");
    const avatarColourChanged = appearanceLivePreviewMarkup({ ...draft, colour: "#b48cf2" }, "#2ac0f0");

    expect(accentChanged).not.toBe(initial);
    expect(nameChanged).not.toBe(initial);
    expect(vibeChanged).not.toBe(initial);
    expect(avatarColourChanged).not.toBe(initial);
    expect(proofCounts(initial)).toEqual({ Verified: 0, "verified-via": 0, connectedServiceProof: 0 });
    expect((initial.match(new RegExp(SAFETY_NUMBER_AUTHENTICATION_COPY, "gu")) ?? []).length).toBe(1);
    expect(isHonestPreview(initial)).toBe(true);
    const screen = appearanceSettingsContent(defaultAppearancePreferences, "dark", draft);
    expect(proofCounts(screen)).toEqual({ Verified: 0, "verified-via": 0, connectedServiceProof: 0 });
    expect((screen.match(new RegExp(SAFETY_NUMBER_AUTHENTICATION_COPY, "gu")) ?? []).length).toBe(1);
    console.log("TASK5083 unsaved_accent=changed unsaved_name=changed unsaved_vibe=changed unsaved_avatar_colour=changed Verified=0 verified_via=0 connected_service_proof=0 safety_sentence=1");
  });

  it("fails when a throwaway green Verified row is restored", () => {
    const throwawayBrokenRender = `${appearanceLivePreviewMarkup(draft, "#2ac0f0")}<div class="green Verified">Verified</div>`;
    expect(throwawayBrokenRender.match(/<div class="green Verified">Verified<\/div>/u)?.length).toBe(1);
    expect(isHonestPreview(throwawayBrokenRender)).toBe(false);
    console.log("TASK5083B throwaway_green_Verified=1 honest_check=false restored_real_render=true");
  });
});
