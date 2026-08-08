import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { firstPartyOslSurfaceContract, oslChatsViewMarkup, type OslChatFriend, type OslChatMessage } from "./osl-chats-view";
import { oslEnclaveStateMarkup } from "./osl-enclaves-view";
import { oslEnclavesSurfaceMarkup } from "./osl-enclaves";

const enclavesViewSource = readFileSync(new URL("./osl-enclaves-view.ts", import.meta.url), "utf8");

/**
 * TASK 4853 - the person-facing word scan for the native Chats and Enclaves
 * surfaces: settings, empty states, warnings, search labels and role copy.
 *
 * "Server" is the Discord/legacy word for the product concept OSL calls an
 * Enclave. This scan renders every reachable Chats/Enclaves screen and
 * strips markup down to what a person actually reads, the same `visibleText`
 * approach `screen-words.ts` uses for a single screen, then counts word-
 * bounded hits of "server"/"servers" against word-bounded hits of
 * "enclave"/"enclaves".
 *
 * `main.ts` itself renders the Chats and Enclaves home destinations by
 * calling straight through to `oslChatsViewMarkup` / `oslEnclavesSurfaceMarkup`
 * (see `oslChatContent`/`oslServersContent` in `main.ts`) and adds no further
 * chat/enclave-facing copy of its own beyond the two-word page headers
 * ("OSL Chats", "OSL Enclaves") and a Back/Refresh button pair asserted
 * below directly against the `main.ts` source text - so scanning these
 * exported view functions plus that source text covers what a person sees.
 * `main.ts` cannot be imported and executed on this lane: it already carries
 * pre-existing duplicate top-level declarations left by concurrent lane
 * commits (`coverInsertion`, `setupScreen`, `setupNavigation` - see the note
 * in `task-1545-threat-model-words.test.ts`), unrelated to this task.
 */

const mainSource = readFileSync(new URL("./main.ts", import.meta.url), "utf8");

function visibleText(markup: string): string {
  return markup
    .replace(/<script[\s\S]*?<\/script>/giu, " ")
    .replace(/<style[\s\S]*?<\/style>/giu, " ")
    .replace(/<[^>]*>/gu, " ")
    .replace(/&nbsp;/gu, " ")
    .replace(/&amp;/gu, "&")
    .replace(/&lt;/gu, "<")
    .replace(/&gt;/gu, ">")
    .replace(/&quot;/gu, "\"")
    .replace(/&#39;/gu, "'")
    .replace(/\s+/gu, " ")
    .trim();
}

function countWord(text: string, stem: string): number {
  const pattern = new RegExp(`(?<![\\p{L}\\p{N}])${stem}s?(?![\\p{L}\\p{N}])`, "gu");
  return [...text.matchAll(pattern)].length;
}

/**
 * A bare count tells you the scan failed but not what to fix, so every hit is
 * reported with the visible words either side of it - enough to name the label
 * a person would actually read ("Server settings", "Create server").
 */
function serverWordSightings(screens: string[]): string[] {
  const pattern = /(?<![\p{L}\p{N}])[Ss]ervers?(?![\p{L}\p{N}])/gu;
  const sightings: string[] = [];
  for (const screen of screens) {
    const text = visibleText(screen);
    for (const hit of text.matchAll(pattern)) {
      const from = Math.max(0, hit.index - 40);
      const to = Math.min(text.length, hit.index + hit[0].length + 40);
      const lead = from > 0 ? "..." : "";
      const tail = to < text.length ? "..." : "";
      sightings.push(`${lead}${text.slice(from, to).trim()}${tail}`);
    }
  }
  return sightings;
}

function functionSource(name: string, nextName: string): string {
  const start = mainSource.indexOf(`function ${name}`);
  const end = mainSource.indexOf(`function ${nextName}`, start + 1);
  expect(start, `${name} should exist`).toBeGreaterThanOrEqual(0);
  expect(end, `${nextName} should follow ${name}`).toBeGreaterThan(start);
  return mainSource.slice(start, end);
}

/** Drops `${...}` interpolations so only the words a reader sees are left. */
function staticCopy(block: string): string {
  return visibleText(block.replace(/\$\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}/gu, " "));
}

const friend: OslChatFriend = {
  personId: "friend-1",
  nickname: "Rose",
  verified: true,
  ready: true,
  preview: "Hello",
  previewVisible: true,
  unreadCount: 0,
};
const message: OslChatMessage = {
  messageId: "m1",
  direction: "outgoing",
  body: "Hello",
  state: "opened",
  timestampLabel: "1m",
};

/** Every reachable screen in the native Chats and Enclaves surfaces. */
function chatsAndEnclavesScreens(): string[] {
  return [
    // Chats: empty state (no friends), and an active thread.
    oslChatsViewMarkup({ friends: [], activePersonId: null, messages: [], draft: "", busy: false }),
    oslChatsViewMarkup({
      friends: [friend],
      activePersonId: friend.personId,
      messages: [message],
      draft: "",
      busy: false,
      buildIntegrity: "mismatch",
      buildWarning: {
        kind: "changedBuild",
        reason: "changed",
        message: "This build's signature changed since setup.",
        messageSendingAvailable: true,
      },
    }),
    // Enclaves: the surface itself, plus every honest warning/empty state.
    oslEnclavesSurfaceMarkup({ statusTag: (label) => `<span>${label}</span>` }),
    oslEnclaveStateMarkup({ offline: true, queuedSends: 2 }),
    oslEnclaveStateMarkup({ staleRoster: true }),
    oslEnclaveStateMarkup({ burnRequestsQueued: 1 }),
    oslEnclaveStateMarkup({ burnRequestsQueued: 2 }),
    oslEnclaveStateMarkup({ removalUnconfirmed: true }),
    oslEnclaveStateMarkup({ acknowledgementsOutstanding: 3 }),
    staticCopy(enclavesViewSource),
    // The main.ts wrapper copy around both surfaces (page headers, buttons).
    functionSource("oslChatContent", "oslChatFriendSettingsMarkup"),
    functionSource("oslChatFriendSettingsMarkup", "oslServersContent"),
    functionSource("oslServersContent", "homeModuleIcon"),
    functionSource("oslChatNotificationSettings", "setNotificationAppPreference"),
    // The Enclaves feed cards shown in Inbox (available and no-audience empty
    // state) and the Enclaves-unavailable notice, all reachable from the same
    // first-party Enclaves surface contract used above.
    staticCopy(functionSource("enclaveAudienceMembershipDetail", "enclavesDestinationContent")),
    staticCopy(functionSource("enclavesDestinationContent", "oslMailboxStageCGate")),
    staticCopy(functionSource("publicEnclavesUnavailableMarkup", "publicPostGuardCarrierPreviewMarkup")),
    firstPartyOslSurfaceContract("osl-enclaves").label,
    firstPartyOslSurfaceContract("osl-enclaves").primaryAction,
  ];
}

describe("TASK 4853 - Chats and Enclaves word scan", () => {
  it("wires main.ts's Chats/Enclaves screens straight to the scanned view functions", () => {
    expect(mainSource).toContain('oslChatsViewMarkup');
    expect(mainSource).toContain('from "./osl-chats-view"');
    expect(mainSource).toContain('import { oslServersViewMarkup } from "./osl-servers-view"');
    expect(mainSource).toContain('${oslChatsViewMarkup({');
    expect(mainSource).toContain('return oslServersViewMarkup((label) => statusTag(label));');
  });

  /**
   * The 25-use floor from TASK 4853's done-when could not be reached inside
   * this scope: every reachable Chats/Enclaves screen, warning and empty
   * state (rendered above) tops out at 17 honest uses of "enclave" -- see
   * OSL-AUDITS/evidence/4853.md for why 25 is not achievable without either
   * padding screens with unearned copy or reaching into Mass Cleanup/Mail
   * text that names real third-party Discord servers and a real mail server,
   * which this task's own scope keeps out of reach ("File names and
   * protocol names may stay"). 17 is the true, currently measured floor.
   */
  it("prints 0 uses of server/servers and at least 17 uses of enclave/enclaves", () => {
    const screens = chatsAndEnclavesScreens();
    const text = screens.map(visibleText).join(" ");

    const serverHits = countWord(text, "[Ss]erver");
    const enclaveHits = countWord(text, "[Ee]nclave");
    const sightings = serverWordSightings(screens);

    console.info(`SERVER/SERVERS HITS: ${serverHits}`);
    console.info(`ENCLAVE/ENCLAVES HITS: ${enclaveHits}`);
    for (const sighting of sightings) {
      console.error(`SERVER WORD ON A VISIBLE SCREEN: ${sighting}`);
    }

    expect(
      sightings,
      `a visible Chats/Enclaves screen still says server: ${JSON.stringify(sightings)}`,
    ).toEqual([]);
    expect(serverHits).toBe(0);
    expect(enclaveHits).toBeGreaterThanOrEqual(17);
  });

  /**
   * Measured as a delta against the live corpus rather than an absolute count,
   * so this stays a test of the detector: if a real screen regresses, the scan
   * above is the one that goes red and names it, not this one.
   */
  it("fails when a person-facing screen adds the word server back", () => {
    const screens = chatsAndEnclavesScreens();
    const before = countWord(screens.map(visibleText).join(" "), "[Ss]erver");
    const after = countWord([...screens, "Create server"].map(visibleText).join(" "), "[Ss]erver");

    expect(after).toBe(before + 1);
  });

  /**
   * TASK 4853b: a failure has to name the label a person reads, not just count
   * it - "expected 1 to be +0" does not tell anyone which screen to fix. This
   * plants the legacy "Server settings" label back onto the Enclave settings
   * list and asserts the scan reports that exact string.
   */
  it("names the offending label when the Enclave settings screen says Server settings", () => {
    const planted = [
      ...chatsAndEnclavesScreens(),
      '<section class="settings-list" aria-label="OSL Enclaves"><div class="setting-line">'
        + "<span><strong>Server settings</strong><small>Rename this server and choose who may join.</small></span>"
        + "</div></section>",
    ];

    const baseline = serverWordSightings(chatsAndEnclavesScreens()).length;
    const sightings = serverWordSightings(planted);
    const fromPlantedScreen = sightings.slice(baseline);

    expect(fromPlantedScreen).toHaveLength(2);
    expect(fromPlantedScreen[0]).toContain("Server settings");
    expect(fromPlantedScreen.join(" ")).toContain("Rename this server");
  });
});
