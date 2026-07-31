import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const ui = readFileSync(new URL("./main.ts", import.meta.url), "utf8");
const rust = readFileSync(new URL("../../osl-hub/src/main.rs", import.meta.url), "utf8");
const security = readFileSync(new URL("../../osl-hub/src/security.rs", import.meta.url), "utf8");

function functionSlice(source: string, name: string, next: string): string {
  const start = source.indexOf(name);
  const end = source.indexOf(next, start + name.length);
  expect(start).toBeGreaterThanOrEqual(0);
  expect(end).toBeGreaterThan(start);
  return source.slice(start, end);
}

describe("atomic OSL profile and username ownership", () => {
  it("writes locally before claiming and restores the old profile on a known rejection", () => {
    const save = functionSlice(rust, "async fn save_osl_profile", "fn active_unlocked_osl_user_id");
    expect(save.indexOf("save_active_profile")).toBeLessThan(save.indexOf("claim_username"));
    expect(save).toContain("restore_active_profile(&owner, previous)");
    expect(save).toContain("lookup_username(&saved.username_candidate)");
  });

  it("uses only read-only ownership status during startup", () => {
    const refresh = functionSlice(ui, "async function refreshIdentityScopedState", "async function submitPasswordRole");
    const readyLoads = functionSlice(ui, "function startReadyWorkspaceLoads", "function scheduleOslChatBackgroundSync");
    expect(refresh).toContain("getOslUsernameStatus");
    expect(readyLoads).toContain("getOslUsernameStatus");
    expect(refresh).not.toContain("claimOslUsername");
    expect(readyLoads).not.toContain("claimOslUsername");
  });

  it("keeps username changes server-atomic and refuses self-friending", () => {
    expect(rust).toContain("claim_username(&identity, &saved.username_candidate");
    expect(security).toContain("OSL refuses to add the active identity as a friend");
  });
});
