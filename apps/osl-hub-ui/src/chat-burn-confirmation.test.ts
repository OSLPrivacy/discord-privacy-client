import { describe, expect, it } from "vitest";
import {
  BURN_SIDES,
  applyChatBurnConfirmationEvent,
  burnScopeChoices,
  burnSummarySentence,
  canConfirmBurn,
  chatBurnConfirmationMarkup,
  chatBurnEventForAction,
  chatBurnRequestDto,
  chatBurnScopeInput,
  initialChatBurnConfirmation,
  insideServer,
  type BurnPlace,
} from "./chat-burn-confirmation";

/** A channel inside a server: the fixture TASK 1355's finish line names. */
const SERVER_PLACE: BurnPlace = {
  kind: "channel",
  id: "channel-general",
  name: "#general",
  serverId: "server-study-hall",
  serverName: "Study Hall",
  serviceId: "osl-chat",
  serviceName: "OSL Chat",
  accountId: "account-task-1355",
  accountName: "you@osl",
};

const DM_PLACE: BurnPlace = {
  kind: "direct_message",
  id: "dm-ada",
  name: "Ada",
  serviceId: "osl-chat",
  serviceName: "OSL Chat",
  accountId: "account-task-1355",
  accountName: "you@osl",
};

describe("TASK 1355 chat burn confirmation", () => {
  it("offers both the channel and the whole server inside a server", () => {
    expect(insideServer(SERVER_PLACE)).toBe(true);
    const scopes = burnScopeChoices(SERVER_PLACE);
    expect(scopes.map((choice) => choice.scope)).toEqual([
      "channel",
      "server",
      "service",
      "account",
    ]);
    const channel = scopes.find((choice) => choice.scope === "channel");
    const server = scopes.find((choice) => choice.scope === "server");
    expect(channel).toMatchObject({ id: "channel-general", label: "This channel", target: "#general" });
    expect(server).toMatchObject({ id: "server-study-hall", label: "Whole server", target: "Study Hall" });
  });

  it("does not offer a server-wide burn outside a server", () => {
    expect(insideServer(DM_PLACE)).toBe(false);
    expect(burnScopeChoices(DM_PLACE).map((choice) => choice.scope)).toEqual([
      "direct_message",
      "service",
      "account",
    ]);
  });

  it("offers the thread, its channel and the whole server from a thread", () => {
    const scopes = burnScopeChoices({
      ...SERVER_PLACE,
      kind: "thread",
      id: "thread-task-1355",
      name: "Reading list",
      channelId: "channel-general",
      channelName: "#general",
      parentMessageId: "parent-message-task-1355",
    });
    expect(scopes.map((choice) => choice.scope)).toEqual([
      "thread",
      "channel",
      "server",
      "service",
      "account",
    ]);
  });

  it("offers exactly the three sides the broker OslChatBurnChoice defines", () => {
    expect(BURN_SIDES).toEqual(["yourSide", "theirSide", "bothSides"]);
  });

  it("chooses nothing by default and keeps Confirm unavailable until both are chosen", () => {
    let state = initialChatBurnConfirmation(SERVER_PLACE);
    expect(state.scope).toBeNull();
    expect(state.side).toBeNull();
    expect(state.hideOthers).toBe(false);
    expect(canConfirmBurn(state)).toBe(false);

    state = applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "server" });
    expect(canConfirmBurn(state)).toBe(false);
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-side", side: "bothSides" });
    expect(canConfirmBurn(state)).toBe(true);
  });

  it("ignores a Confirm sent before a scope and a side are chosen", () => {
    const state = initialChatBurnConfirmation(SERVER_PLACE);
    expect(applyChatBurnConfirmationEvent(state, { kind: "confirm" })).toBe(state);
    const scoped = applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "channel" });
    expect(applyChatBurnConfirmationEvent(scoped, { kind: "confirm" }).outcome).toBe("choosing");
  });

  it("ignores scopes this place does not offer", () => {
    const state = initialChatBurnConfirmation(DM_PLACE);
    expect(applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "server" })).toBe(state);
    expect(applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "channel" })).toBe(state);
    expect(applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "direct_message" }).scope)
      .toBe("direct_message");
  });

  it("Back leaves without burning and freezes the screen", () => {
    let state = initialChatBurnConfirmation(SERVER_PLACE);
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "server" });
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-side", side: "bothSides" });
    const backed = applyChatBurnConfirmationEvent(state, { kind: "back" });
    expect(backed.outcome).toBe("went-back");
    expect(applyChatBurnConfirmationEvent(backed, { kind: "confirm" }).outcome).toBe("went-back");
    expect(canConfirmBurn(backed)).toBe(false);
  });

  it("maps clicked controls to events and everything else to null", () => {
    expect(chatBurnEventForAction("scope", "server")).toEqual({ kind: "choose-scope", scope: "server" });
    expect(chatBurnEventForAction("side", "theirSide")).toEqual({ kind: "choose-side", side: "theirSide" });
    expect(chatBurnEventForAction("hide-others", "on")).toEqual({ kind: "set-hide-others", hide: true });
    expect(chatBurnEventForAction("hide-others", "off")).toEqual({ kind: "set-hide-others", hide: false });
    expect(chatBurnEventForAction("back", null)).toEqual({ kind: "back" });
    expect(chatBurnEventForAction("confirm", null)).toEqual({ kind: "confirm" });
    expect(chatBurnEventForAction("side", "allSides")).toBeNull();
    expect(chatBurnEventForAction(null, null)).toBeNull();
  });

  it("produces the TASK 1351 scope input for the channel and the whole server", () => {
    expect(chatBurnScopeInput(SERVER_PLACE, "channel")).toEqual({
      scopeKind: "channel",
      scopeId: "channel-general",
      serviceId: "osl-chat",
      accountId: "account-task-1355",
      serverId: "server-study-hall",
      channelId: "channel-general",
    });
    expect(chatBurnScopeInput(SERVER_PLACE, "server")).toEqual({
      scopeKind: "server",
      scopeId: "server-study-hall",
      serviceId: "osl-chat",
      accountId: "account-task-1355",
      serverId: "server-study-hall",
    });
    expect(chatBurnScopeInput(DM_PLACE, "server")).toBeNull();
  });

  it("produces the exact burn_osl_chat_history arguments after Confirm", () => {
    let state = initialChatBurnConfirmation(SERVER_PLACE);
    expect(chatBurnRequestDto(state)).toBeNull();
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "server" });
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-side", side: "bothSides" });
    state = applyChatBurnConfirmationEvent(state, { kind: "set-hide-others", hide: true });
    state = applyChatBurnConfirmationEvent(state, { kind: "confirm" });
    expect(state.outcome).toBe("confirmed");
    expect(chatBurnRequestDto(state)).toEqual({
      choice: "bothSides",
      hideOthersMessages: true,
      scope: {
        scopeKind: "server",
        scopeId: "server-study-hall",
        serviceId: "osl-chat",
        accountId: "account-task-1355",
        serverId: "server-study-hall",
      },
    });
  });

  it("says in one sentence exactly what Confirm will do", () => {
    let state = initialChatBurnConfirmation(SERVER_PLACE);
    expect(burnSummarySentence(state)).toBe("Choose how much to burn and whose copies it reaches.");
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "channel" });
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-side", side: "yourSide" });
    expect(burnSummarySentence(state)).toBe("Confirm burns this channel (#general), your side.");
    state = applyChatBurnConfirmationEvent(state, { kind: "set-hide-others", hide: true });
    expect(burnSummarySentence(state)).toBe(
      "Confirm burns this channel (#general), your side. Other people's messages are also hidden from your view.",
    );
  });

  it("draws all five controls, with both server scopes and all three sides", () => {
    const markup = chatBurnConfirmationMarkup(initialChatBurnConfirmation(SERVER_PLACE));
    expect(markup).toContain('data-burn-action="scope" data-burn-value="channel"');
    expect(markup).toContain('data-burn-action="scope" data-burn-value="server"');
    for (const side of BURN_SIDES) {
      expect(markup).toContain(`data-burn-action="side" data-burn-value="${side}"`);
    }
    expect(markup).toContain('id="chat-burn-hide-others"');
    expect(markup).toContain('id="chat-burn-back"');
    expect(markup).toContain('id="chat-burn-confirm"');
    expect(markup).toMatch(/id="chat-burn-confirm"[^>]*disabled/u);
    expect(markup).toContain("This channel");
    expect(markup).toContain("Whole server");
    expect(markup).toContain("Study Hall");
  });

  it("enables Confirm in the markup once a scope and a side are pressed", () => {
    let state = initialChatBurnConfirmation(SERVER_PLACE);
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-scope", scope: "server" });
    state = applyChatBurnConfirmationEvent(state, { kind: "choose-side", side: "bothSides" });
    const markup = chatBurnConfirmationMarkup(state);
    expect(markup).not.toMatch(/id="chat-burn-confirm"[^>]*disabled/u);
    expect(markup).toContain('data-burn-value="server" aria-pressed="true"');
    expect(markup).toContain('data-burn-value="bothSides" aria-pressed="true"');
    expect(markup).toContain('data-burn-value="channel" aria-pressed="false"');
  });

  it("never claims a burn un-sends, and says hiding is not deleting", () => {
    const markup = chatBurnConfirmationMarkup(initialChatBurnConfirmation(SERVER_PLACE));
    expect(markup).toContain("Burn cleans up. It does not un-send.");
    expect(markup).toContain("It does not delete anyone else's messages.");
    expect(markup).not.toMatch(/gone for good|forever|unrecoverable|guaranteed/iu);
  });
});
