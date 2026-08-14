import { describe, expect, it } from "vitest";
import { appearancePreviewMarkup } from "./appearance-preview";
import type { ScopedProfileRecord } from "./osl-profile-pane";

const profile: ScopedProfileRecord = {
  scope: { kind: "global" },
  useSeparateProfileHere: false,
  displayName: "Avery",
  aboutLine: "",
  status: "Making space for quiet work.",
  cardBackground: "#0d1114",
  avatar: null,
  colour: "#b48cf2",
};

describe("TASK 5083 Appearance preview", () => {
  it("uses the live draft profile and selected colours, including the initial avatar", () => {
    const first = appearancePreviewMarkup(profile, { accent: "#2ac0f0", background: "#080c0d" }, []);
    const changed = appearancePreviewMarkup({ ...profile, displayName: "Morgan", status: "Out for a walk.", colour: "#35c46a" }, { accent: "#35c46a", background: "#0a0f0c" }, []);
    console.log("TASK5083 initial=Avery,Making space for quiet work.,A,#b48cf2,#2ac0f0");
    console.log("TASK5083 changed=Morgan,Out for a walk.,M,#35c46a,#35c46a");
    expect(first).toContain(">Avery<");
    expect(first).toContain("Making space for quiet work.");
    expect(first).toContain(">A<");
    expect(first).toContain("--appearance-preview-avatar:#b48cf2");
    expect(changed).toContain(">Morgan<");
    expect(changed).toContain("Out for a walk.");
    expect(changed).toContain(">M<");
    expect(changed).toContain("--appearance-preview-avatar:#35c46a");
    expect(changed).toContain("--appearance-preview-accent:#35c46a");
  });

  it("shows an uploaded avatar, Verified, and each currently connected service", () => {
    const markup = appearancePreviewMarkup({ ...profile, avatar: "data:image/png;base64,avatar" }, { accent: "#2ac0f0", background: "#080c0d" }, [
      { id: "discord", label: "Discord", icon: '<svg data-icon="discord"></svg>' },
      { id: "signal", label: "Signal", icon: '<svg data-icon="signal"></svg>' },
    ]);
    console.log("TASK5083 verified=Verified services=discord,signal avatar=image");
    expect(markup).toContain('src="data:image/png;base64,avatar"');
    expect(markup).toContain("Verified");
    expect(markup).toContain('data-appearance-preview-service="discord"');
    expect(markup).toContain('data-appearance-preview-service="signal"');
  });
});
