import { describe, expect, it } from "vitest";
import { isUserAllowed } from "../../src/lib/auth.js";
import type { Env } from "../../src/env.js";

// Cheap fake — only the OSL_KEYSERVER_ALLOWED_USERS field matters
// for this test; the rest are unused by isUserAllowed.
function envWithAllowlist(csv: string | undefined): Env {
  return {
    DB: {} as D1Database,
    RATE_LIMIT_KV: {} as KVNamespace,
    OSL_KEYSERVER_ADMIN_TOKEN: "x",
    OSL_KEYSERVER_ALLOWED_USERS: csv,
  };
}

describe("isUserAllowed", () => {
  it("returns true when no allowlist is configured", () => {
    expect(isUserAllowed(envWithAllowlist(undefined), "anyone")).toBe(true);
    expect(isUserAllowed(envWithAllowlist(""), "anyone")).toBe(true);
  });

  it("returns true for entries on the CSV allowlist", () => {
    const env = envWithAllowlist("liam,henry");
    expect(isUserAllowed(env, "liam")).toBe(true);
    expect(isUserAllowed(env, "henry")).toBe(true);
  });

  it("returns false for entries not on the allowlist", () => {
    const env = envWithAllowlist("liam,henry");
    expect(isUserAllowed(env, "mallory")).toBe(false);
  });

  it("trims whitespace around CSV entries", () => {
    const env = envWithAllowlist("  liam , henry  ");
    expect(isUserAllowed(env, "liam")).toBe(true);
    expect(isUserAllowed(env, "henry")).toBe(true);
  });

  it("treats an all-whitespace allowlist as no allowlist", () => {
    const env = envWithAllowlist("   ,  ,   ");
    expect(isUserAllowed(env, "anyone")).toBe(true);
  });
});
