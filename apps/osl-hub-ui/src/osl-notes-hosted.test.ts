import { describe, expect, it } from "vitest";
import { createOslHostedWorkspaceAccess, hostedNotesAvailability } from "./osl-notes-hosted";

describe("hosted Notes boundary", () => {
  it("creates separate opaque locator and capability values while staying honestly disabled", () => {
    const access = createOslHostedWorkspaceAccess();
    expect(access.roomLocator).toMatch(/^[A-Za-z0-9_-]{43}$/u);
    expect(access.roomCapability).toMatch(/^[A-Za-z0-9_-]{43}$/u);
    expect(access.roomLocator).not.toBe(access.roomCapability);
    expect(hostedNotesAvailability.available).toBe(false);
  });
});
