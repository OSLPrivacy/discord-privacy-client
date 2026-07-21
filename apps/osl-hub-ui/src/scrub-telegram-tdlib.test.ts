import { describe, expect, it, vi } from "vitest";
import { TELEGRAM_TDLIB_CAPABILITY, TdlibDeleteOnlyClient, type TdlibDeleteOnlyJsonClient } from "./scrub-telegram-tdlib";

const message = { "@type": "message", id: "42", chat_id: "10", date: 100, is_outgoing: true, can_be_deleted_for_all_users: true, content: { "@type": "messageText", text: { text: "private" } } } as const;

describe("TDLib delete-only binding", () => {
  it("is disabled until the real binary and phone session are supplied", () => {
    expect(TELEGRAM_TDLIB_CAPABILITY).toMatchObject({ deletionEnabledByDefault: false, clientBinaryPackaged: false, phoneSessionAuthenticationRequired: true });
  });

  it("maps documented TDLib requests and requires authorizationStateReady", async () => {
    const send = vi.fn(async (request: { "@type": string }) => {
      if (request["@type"] === "getAuthorizationState") return { "@type": "authorizationStateReady" };
      if (request["@type"] === "getChatHistory") return { "@type": "messages", messages: [message] };
      if (request["@type"] === "getMessage") return message;
      return { "@type": "ok" };
    });
    const client = new TdlibDeleteOnlyClient({ send } as TdlibDeleteOnlyJsonClient, () => "local-hash");
    expect(await client.getChatHistory("10", 200_000)).toEqual([{ id: "42", chatId: "10", authoredBySelf: true, createdAtUnixMs: 100_000, contentFingerprint: "local-hash", canBeDeletedForAllUsers: true }]);
    await client.deleteMessages("10", ["42"], true);
    expect(send).toHaveBeenCalledWith({ "@type": "deleteMessages", chat_id: "10", message_ids: ["42"], revoke: true });
  });

  it("does not fake or advance a phone authentication session", async () => {
    const send = vi.fn(async () => ({ "@type": "authorizationStateWaitPhoneNumber" }));
    const client = new TdlibDeleteOnlyClient({ send }, () => "hash");
    await expect(client.getMessage("10", "42")).rejects.toThrow("not authorizationStateReady");
    expect(send).toHaveBeenCalledTimes(1);
    expect(send).toHaveBeenCalledWith({ "@type": "getAuthorizationState" });
  });

  it("propagates provider FLOOD_WAIT errors exactly to the adapter layer", async () => {
    const send = vi.fn(async (request: { "@type": string }) => request["@type"] === "getAuthorizationState"
      ? { "@type": "authorizationStateReady" }
      : { "@type": "error", code: 429, message: "FLOOD_WAIT_37" });
    const client = new TdlibDeleteOnlyClient({ send } as TdlibDeleteOnlyJsonClient, () => "hash");
    await expect(client.deleteMessages("10", ["42"], true)).rejects.toThrow("FLOOD_WAIT_37");
  });

  it("keeps TDLib 404 readback failures ambiguous instead of claiming absence", async () => {
    const send = vi.fn(async (request: { "@type": string }) => request["@type"] === "getAuthorizationState"
      ? { "@type": "authorizationStateReady" }
      : { "@type": "error", code: 404, message: "Message or chat is inaccessible" });
    const client = new TdlibDeleteOnlyClient({ send } as TdlibDeleteOnlyJsonClient, () => "hash");
    await expect(client.getMessage("10", "42")).rejects.toThrow("TDLib 404");
  });

  it("rejects a returned message whose id does not match the requested message", async () => {
    const send = vi.fn(async (request: { "@type": string }) => request["@type"] === "getAuthorizationState"
      ? { "@type": "authorizationStateReady" }
      : { ...message, id: "99" });
    const client = new TdlibDeleteOnlyClient({ send } as TdlibDeleteOnlyJsonClient, () => "hash");
    await expect(client.getMessage("10", "42")).rejects.toThrow("schema drift");
  });

  it("paginates with the last received id, de-duplicates the cursor, and stays bounded", async () => {
    const send = vi.fn(async (request: { "@type": string; from_message_id?: string }) => {
      if (request["@type"] === "getAuthorizationState") return { "@type": "authorizationStateReady" };
      if (request.from_message_id === "0") return { "@type": "messages", messages: [{ ...message, id: "42" }, { ...message, id: "41", date: 90 }] };
      if (request.from_message_id === "41") return { "@type": "messages", messages: [{ ...message, id: "41", date: 90 }, { ...message, id: "40", date: 80 }] };
      return { "@type": "messages", messages: [] };
    });
    const client = new TdlibDeleteOnlyClient({ send } as TdlibDeleteOnlyJsonClient, (item) => `hash-${item.id}`);
    expect((await client.getChatHistory("10", 200_000)).map((item) => item.id)).toEqual(["42", "41", "40"]);
    expect(send).toHaveBeenCalledWith(expect.objectContaining({ "@type": "getChatHistory", from_message_id: "41" }));
    expect(send.mock.calls.filter(([request]) => request["@type"] === "getChatHistory")).toHaveLength(3);
  });
});
