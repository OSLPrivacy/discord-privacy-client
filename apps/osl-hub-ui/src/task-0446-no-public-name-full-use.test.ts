import { beforeEach, describe, expect, it, vi } from "vitest";

const CONTACT_PERSON_ID = "private-contact-0446";
const CONTACT_OSL_ID = "osl-private-contact-0446";
const CONTACT_NAME = "Private Cedar 0446";
const MARKED_MESSAGE = "TASK0446-MARKED-DIRECT-MESSAGE";
const FRIEND_CODE = `OSLFR1.${"C".repeat(32)}`;

type SavedAccount = {
  onboardingComplete: boolean;
  publicName: string | null;
};

type SavedContact = {
  personId: string;
  oslUserId: string;
  alias: string;
  safetyNumber: string;
  safetyNumberVerified: boolean;
  whitelistCount: number;
  whitelistedScopes: unknown[];
  whitelistedScopesTruncated: boolean;
  pendingKeyChange: boolean;
  reachBroadened: boolean;
  reachBroadenedAt: null;
  reachNarrowedScopes: string[];
};

type SavedDirectMessage = {
  messageId: string;
  contactPersonId: string;
  contactName: string;
  text: string;
};

const native = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  emitTo: vi.fn(),
  getCurrentWindow: vi.fn(),
}));

vi.mock("@fontsource-variable/inter/wght.css", () => ({}));
vi.mock("./logos", () => ({
  browserLogo: (id: string) => `<span>${id}</span>`,
  providerLogo: (id: string) => `<span>${id}</span>`,
  serviceLogo: (id: string) => `<span>${id}</span>`,
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: native.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ emitTo: native.emitTo, listen: native.listen }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: native.getCurrentWindow }));

function memoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() { return values.size; },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key); },
    setItem: (key, value) => { values.set(key, value); },
  };
}

function installGlobals(): void {
  const root = {
    innerHTML: "",
    querySelector: vi.fn(() => null),
    querySelectorAll: vi.fn(() => []),
  };
  vi.stubGlobal("localStorage", memoryStorage());
  vi.stubGlobal("document", {
    querySelector: (selector: string) => selector === "#app" ? root : null,
    querySelectorAll: () => [],
    createElement: () => root,
    documentElement: { classList: { add: vi.fn() }, dataset: {} },
    body: { append: vi.fn() },
    addEventListener: vi.fn(),
    activeElement: null,
    visibilityState: "visible",
  });
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {},
    addEventListener: vi.fn(),
    matchMedia: vi.fn(() => ({ matches: false, addEventListener: vi.fn() })),
    setTimeout,
    clearTimeout,
    confirm: vi.fn(() => false),
  });
  vi.stubGlobal("navigator", { onLine: true, clipboard: { writeText: vi.fn() } });
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
}

describe("TASK 0446 no-public-name full use", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
    native.invoke.mockReset();
    installGlobals();
  });

  it("saves no public name, a named private contact, and a direct marked message", async () => {
    const savedAccount: SavedAccount = { onboardingComplete: false, publicName: null };
    const savedContacts: SavedContact[] = [];
    const savedDirectMessages: SavedDirectMessage[] = [];
    let activeContactPersonId: string | null = null;

    native.invoke.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      if (command === "create_hub_private_contact_link") {
        return {
          personId: "account-owner-0446",
          linkValue: `OSLCL1.${"A".repeat(43)}`,
          usesAllowed: 1,
          usesRecorded: 0,
        };
      }
      if (command === "claim_hub_username") {
        savedAccount.publicName = String(args?.username ?? "");
        return { username: savedAccount.publicName, oslUserId: "account-owner-0446" };
      }
      if (command === "save_onboarding_preferences") {
        const preferences = args?.preferences as { onboardingComplete?: boolean } | undefined;
        savedAccount.onboardingComplete = preferences?.onboardingComplete === true;
        return args?.preferences;
      }
      if (command === "add_hub_friend") {
        savedContacts.push({
          personId: CONTACT_PERSON_ID,
          oslUserId: CONTACT_OSL_ID,
          alias: String(args?.alias ?? ""),
          safetyNumber: "0446 0446",
          safetyNumberVerified: true,
          whitelistCount: 1,
          whitelistedScopes: [{ kind: "dm", contextId: null, storageKey: "dm", userSpecific: false }],
          whitelistedScopesTruncated: false,
          pendingKeyChange: false,
          reachBroadened: false,
          reachBroadenedAt: null,
          reachNarrowedScopes: [],
        });
        return { added: true };
      }
      if (command === "list_hub_people") return savedContacts;
      if (command === "activate_osl_chat_context") {
        activeContactPersonId = String(args?.personId ?? "");
        return {
          contextToken: "ctx.task-0446",
          serviceId: "osl-chat",
          accountId: "osl-main",
          personId: activeContactPersonId,
          peerOslUserId: CONTACT_OSL_ID,
          scopeApproved: true,
        };
      }
      if (command === "prepare_osl_chat_text") {
        const contact = savedContacts.find(({ personId }) => personId === activeContactPersonId);
        if (!contact) throw new Error("TASK0446 direct message has no active saved contact");
        const messageId = "direct-message-0446";
        savedDirectMessages.push({
          messageId,
          contactPersonId: contact.personId,
          contactName: contact.alias,
          text: String(args?.plaintext ?? ""),
        });
        return {
          messageId,
          expiresAt: 1_800_000_446,
          personToPersonE2ee: true,
          viewOnce: args?.viewOnce === true,
          deliveredToOslInbox: true,
        };
      }
      throw new Error(`unavailable in focused TASK 0446 harness: ${command}`);
    });

    const ui = await import("./main");
    const { activateOslChatContext, addOslFriend, listHubPeople, prepareOslChatText } = await import("./adapters");
    ui.__oslHubUiTest.reset({
      route: "onboarding",
      onboardingRoute: "identity-choice",
      identityDiscoveryChoice: null,
      coreReady: true,
    });

    await ui.__oslHubUiTest.chooseNoPublicName();
    expect(ui.__oslHubUiTest.continueFromPrivateContactLink()).toBe(true);
    await ui.__oslHubUiTest.finishOnboarding();

    expect(await addOslFriend(FRIEND_CODE, CONTACT_NAME)).toEqual({ added: true, reason: "" });
    const savedPeople = await listHubPeople();
    expect(savedPeople?.map(({ alias }) => alias)).toEqual([CONTACT_NAME]);

    const directContext = await activateOslChatContext(CONTACT_PERSON_ID);
    expect(directContext?.personId).toBe(CONTACT_PERSON_ID);
    expect(await prepareOslChatText(MARKED_MESSAGE)).toMatchObject({
      messageId: "direct-message-0446",
      deliveredToOslInbox: true,
    });

    expect(savedAccount.onboardingComplete).toBe(true);
    expect(
      savedAccount.publicName,
      `TASK0446 saved public-name error: expected none, saved ${String(savedAccount.publicName)}`,
    ).toBeNull();
    expect(localStorage.getItem("osl-identity-discovery-choice-v1")).toBe("private-link");
    expect(savedContacts).toHaveLength(1);
    expect(savedContacts[0]?.alias).toBe(CONTACT_NAME);
    expect(savedDirectMessages).toEqual([{
      messageId: "direct-message-0446",
      contactPersonId: CONTACT_PERSON_ID,
      contactName: CONTACT_NAME,
      text: MARKED_MESSAGE,
    }]);

    const commands = native.invoke.mock.calls.map(([command]) => String(command));
    expect(commands.filter((command) => command === "claim_hub_username")).toHaveLength(0);
    expect(commands.filter((command) => command === "add_hub_friend")).toHaveLength(1);
    expect(commands.filter((command) => command === "prepare_osl_chat_text")).toHaveLength(1);

    console.log(`TASK0446_SAVED_ACCOUNT_PUBLIC_NAME=${savedAccount.publicName ?? "none"}`);
    console.log(`TASK0446_SAVED_CONTACT_NAME=${savedContacts[0]?.alias}`);
    console.log(`TASK0446_SAVED_DIRECT_MESSAGE_CONTACT=${savedDirectMessages[0]?.contactName}`);
    console.log(`TASK0446_SAVED_DIRECT_MESSAGE_TEXT=${savedDirectMessages[0]?.text}`);
  }, 30_000);
});
