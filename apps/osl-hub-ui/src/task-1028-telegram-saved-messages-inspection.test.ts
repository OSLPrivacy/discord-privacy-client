import { describe, expect, it } from "vitest";
import {
  inspectTelegramAllowedPlace,
  telegramSavedMessagesControlsMarkup,
  type TelegramAllowedPlace,
  type TelegramPlaceReader,
  type TelegramVerificationState,
} from "./telegram-whitelist-controls";

const ACCOUNT = "telegram-owner-1028";
const noOpReader = process.env.TASK1028_TELEGRAM_PLACE_READER === "noop";

function fixture(kind: string, allowed = true): TelegramAllowedPlace {
  return {
    app: "telegram",
    account: ACCOUNT,
    kind,
    stableId: `telegram:${ACCOUNT}:${kind}:place-1028`,
    personName: kind === "saved_messages" ? "Me" : "Another Telegram place",
    placeName: kind === "saved_messages" ? "Saved Messages" : `Telegram ${kind}`,
    allowed,
  };
}

function reader(): TelegramPlaceReader {
  return noOpReader ? () => undefined : () => fixture("saved_messages");
}

const reciprocalDirectMessage: TelegramVerificationState = {
  app: "telegram",
  kind: "direct_message",
  firstAccount: ACCOUNT,
  secondAccount: "telegram-peer-1028",
  firstToSecondStableId: `telegram:${ACCOUNT}:direct_message:telegram-peer-1028`,
  secondToFirstStableId: `telegram:telegram-peer-1028:direct_message:${ACCOUNT}`,
  firstToSecondAllowed: true,
  secondToFirstAllowed: true,
  state: "two-way",
};

describe("TASK1028 Telegram Saved Messages inspection", () => {
  it("directly reads saved_messages and returns only its owner-scoped controls", () => {
    const inspected = inspectTelegramAllowedPlace(reader(), reciprocalDirectMessage);

    expect(inspected).not.toBeNull();
    expect(inspected!.kind).toBe("saved_messages");
    expect(inspected!.controls).toContain("data-telegram-saved-messages-controls");
    expect(inspected!.controls).toContain("data-telegram-saved-messages-toggle");
    expect(inspected!.controls).not.toContain("data-telegram-whitelist-controls");
    expect(inspected!.controls).not.toContain("data-telegram-whitelist-toggle");
    expect(inspected!.controls).not.toContain("data-telegram-verification-tick");

    const ownControls = (inspected!.controls.match(/data-telegram-saved-messages-controls/gu) ?? []).length;
    const inheritedControls = (inspected!.controls.match(/data-telegram-(?:whitelist|verification)-/gu) ?? []).length;
    expect(ownControls).toBe(1);
    expect(inheritedControls).toBe(0);
    console.log(`TASK1028_SAVED_MESSAGE_KIND=${inspected!.kind} TASK1028_OWN_CONTROLS=${ownControls} TASK1028_INHERITED_CONTROLS=${inheritedControls}`);
  });

  it("does not give another Telegram kind or an unallowed Saved Messages place its controls", () => {
    const otherPlaces = [
      fixture("direct_message"),
      fixture("group_chat"),
      fixture("channel"),
      fixture("supergroup"),
      fixture("saved_messages", false),
    ];

    expect(otherPlaces.map(telegramSavedMessagesControlsMarkup)).toEqual(["", "", "", "", ""]);
    console.log(`TASK1028_OTHER_PLACE_OWN_CONTROLS=${otherPlaces.filter((place) => telegramSavedMessagesControlsMarkup(place) !== "").length}`);
  });
});
