import { describe, expect, it } from "vitest";
import { AndroidSurface } from "./services";

function validAndroidSurfaceCatalog(): unknown[] {
  return [
    {
      id: "androidMobileWorkspace",
      displayName: "Android Mobile Workspace",
      surface: "mobileWorkspace",
      launchState: "comingSoon",
      entitlement: "pro",
      defaultEnabled: false,
      consent: "required",
      binding: "localWorkspace",
      authority: "userActionOnly",
      hostedExecution: false,
      workspace: {
        runtime: "localVirtualDevice",
        encryptedDisk: true,
        snapshotStorage: "encryptedLocalOnly",
        wipeKey: "perOslIdentity",
        clipboard: "denied",
        files: "denied",
        notifications: "denied",
        camera: "denied",
        microphone: "denied",
        location: "denied",
      },
    },
    {
      id: "androidCompanion",
      displayName: "Android Companion",
      surface: "companion",
      launchState: "comingSoon",
      entitlement: "included",
      defaultEnabled: false,
      consent: "required",
      binding: "pairedDevice",
      authority: "userActionOnly",
      hostedExecution: false,
      workspace: null,
    },
  ];
}

describe("AndroidSurface", () => {
  it("models the phone companion separately from the mobile workspace", () => {
    const surfaces = AndroidSurface.preview();

    expect(surfaces.map((surface) => [surface.id, surface.surface, surface.entitlement])).toEqual([
      ["androidCompanion", "companion", "included"],
      ["androidMobileWorkspace", "mobileWorkspace", "pro"],
    ]);
    expect(surfaces[0].workspace).toBeNull();
    expect(surfaces[1].workspace).toMatchObject({
      runtime: "localVirtualDevice",
      encryptedDisk: true,
      clipboard: "denied",
      files: "denied",
      notifications: "denied",
      camera: "denied",
      microphone: "denied",
      location: "denied",
    });
  });

  it("normalizes the exact two-surface backend catalog without merging identities", () => {
    const parsed = AndroidSurface.parse(validAndroidSurfaceCatalog());

    expect(parsed.map((surface) => surface.id)).toEqual(["androidCompanion", "androidMobileWorkspace"]);
    expect(parsed[0]).toMatchObject({ displayName: "Android Companion", binding: "pairedDevice", hostedExecution: false });
    expect(parsed[1]).toMatchObject({ displayName: "Android Mobile Workspace", binding: "localWorkspace", hostedExecution: false });
  });

  it("refuses missing consent, missing binding, broad authority, and enabled workspace permissions", () => {
    const missingConsent = validAndroidSurfaceCatalog();
    delete (missingConsent[0] as Record<string, unknown>).consent;
    expect(() => AndroidSurface.parse(missingConsent)).toThrow("invalid Android surface catalog");

    const missingBinding = validAndroidSurfaceCatalog();
    delete (missingBinding[1] as Record<string, unknown>).binding;
    expect(() => AndroidSurface.parse(missingBinding)).toThrow("invalid Android surface catalog");

    const broadAuthority = validAndroidSurfaceCatalog();
    (broadAuthority[0] as Record<string, unknown>).authority = "backgroundControl";
    expect(() => AndroidSurface.parse(broadAuthority)).toThrow("invalid Android surface catalog");

    const enabledClipboard = validAndroidSurfaceCatalog();
    (((enabledClipboard[0] as Record<string, unknown>).workspace as Record<string, unknown>).clipboard) = "userEnabled";
    expect(() => AndroidSurface.parse(enabledClipboard)).toThrow("invalid Android surface catalog");
  });

  it("rejects extra backend data and any attempt to treat the workspace as hosted", () => {
    const withAccountData = validAndroidSurfaceCatalog();
    (withAccountData[1] as Record<string, unknown>).accountIdentifier = "private-account";
    expect(() => AndroidSurface.parse(withAccountData)).toThrow("invalid Android surface catalog");

    const hosted = validAndroidSurfaceCatalog();
    (hosted[0] as Record<string, unknown>).hostedExecution = true;
    expect(() => AndroidSurface.parse(hosted)).toThrow("invalid Android surface catalog");
  });
});
