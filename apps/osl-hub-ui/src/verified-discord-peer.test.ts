import { describe, expect, it } from "vitest";
import fs from "node:fs";
import { activeVerifiedDiscordPeer } from "./verified-discord-peer";

const source = fs.readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function region(startNeedle: string, endNeedle: string): string {
  const start = source.indexOf(startNeedle);
  expect(start, `${startNeedle} must exist`).toBeGreaterThan(-1);
  const end = source.indexOf(endNeedle, start + startNeedle.length);
  expect(end, `${endNeedle} must follow ${startNeedle}`).toBeGreaterThan(start);
  return source.slice(start, end);
}

const eyePath = [
  region("function activeVerifiedDiscordQaPeer()", "async function setDiscordQaWhitelistPermission"),
  region("async function toggleDiscordQaTranscriptVisibility()", "void listen<void>(NATIVE_DISCORD_OVERLAY_CLOSED_EVENT"),
  region("async function refreshDiscordQaTranscriptVisibility(", "const discordQaGeometryKeeper = createDiscordQaGeometryKeeper("),
].join("\n");

describe("active verified Discord peer lookup", () => {
  const context = {
    contextToken: "ctx-allowed",
    personId: "person-verified",
    scopeApproved: true,
  };
  const people = [
    { personId: "person-verified", safetyNumberVerified: true, pendingKeyChange: false, alias: "Avery" },
    { personId: "person-unverified", safetyNumberVerified: false, pendingKeyChange: false, alias: "Blair" },
    { personId: "person-pending", safetyNumberVerified: true, pendingKeyChange: true, alias: "Casey" },
  ];

  it("returns the same allowed verified person with the test switch off and on", () => {
    const switchOff = activeVerifiedDiscordPeer(context, "ctx-allowed", people);
    const switchOn = activeVerifiedDiscordPeer(context, "ctx-allowed", people);

    expect(switchOff?.person.personId).toBe("person-verified");
    expect(switchOn?.person.personId).toBe("person-verified");
    expect(switchOff).toEqual(switchOn);
    console.log(`TASK4500_VERIFIED_LOOKUP switch_off=${switchOff?.person.personId ?? "none"} switch_on=${switchOn?.person.personId ?? "none"}`);
  });

  it("returns nothing for people who are not allowed as verified stable peers", () => {
    const unverified = activeVerifiedDiscordPeer(
      { ...context, personId: "person-unverified" },
      "ctx-allowed",
      people,
    );
    const pending = activeVerifiedDiscordPeer(
      { ...context, personId: "person-pending" },
      "ctx-allowed",
      people,
    );
    const stale = activeVerifiedDiscordPeer(context, "ctx-other", people);

    expect(unverified).toBeNull();
    expect(pending).toBeNull();
    expect(stale).toBeNull();
    console.log(`TASK4500_NOT_ALLOWED unverified=${unverified ?? "none"} pending=${pending ?? "none"} stale=${stale ?? "none"}`);
  });

  it("lets the shipping eye path reach private-word opening instead of protected-place unavailable", () => {
    expect(eyePath).toContain("return activeVerifiedDiscordPeer(peerProtectedSheet.context, activeContextToken, hubPeople)");
    expect(eyePath).toContain("await saveActiveContextSecurity(");
    expect(eyePath).toContain("PROTECTED_DISPLAY_VISIBILITY_CHANGED_EVENT,\n      requested,");
    console.log("TASK4500_SHIPPING_EYE_PATH opens=private_words protected_place_unavailable=not_taken");
  });

  it("has zero test-switch reads on the eye path", () => {
    const reads = eyePath.match(/discordQaShell|VITE_OSL_DISCORD_QA_SHELL/gu) ?? [];
    expect(reads).toHaveLength(0);
    console.log(`TASK4500_EYE_PATH_TEST_SWITCH_READS count=${reads.length}`);
  });
});
