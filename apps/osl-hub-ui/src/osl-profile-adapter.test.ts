import { describe, expect, it } from "vitest";
import { parseOslProfile } from "./adapters";

const profile = {
  displayName: "Liam",
  usernameCandidate: "liam_dev",
  avatar: "https://example.com/avatar.gif",
  accentColor: "#06b6d4",
  bannerColor: "#141414",
  frame: "thin",
  effect: "gradient",
  status: "Building OSL",
};

describe("OSL profile adapter", () => {
  it("accepts the exact bounded backend shape", () => {
    expect(parseOslProfile(profile)).toEqual(profile);
  });

  it("rejects unsafe avatars, invalid handles, and extra fields", () => {
    expect(parseOslProfile({ ...profile, avatar: "javascript:alert(1)" })).toBeNull();
    expect(parseOslProfile({ ...profile, usernameCandidate: "bad name" })).toBeNull();
    expect(parseOslProfile({ ...profile, usernameCandidate: "bad.name" })).toBeNull();
    expect(parseOslProfile({ ...profile, admin: true })).toBeNull();
  });

  it("accepts a bounded GIF data URL", () => {
    expect(parseOslProfile({ ...profile, avatar: "data:image/gif;base64,R0lGODlhAQABAAAAACw=" })?.avatar).toContain("image/gif");
  });
});
