import { describe, expect, it } from "vitest";
import {
  inspectTelegramAllowedPlace,
  type TelegramAllowedPlace,
  type TelegramPlaceKind,
  type TelegramPlaceReader,
  type TelegramVerificationState,
} from "./telegram-whitelist-controls";

const ACCOUNT = "telegram-alice-1026";
const noOpReader = process.env.TASK1026_TELEGRAM_PLACE_READER === "noop";

function fixture(kind: TelegramPlaceKind): TelegramAllowedPlace {
  const placeId = kind === "direct_message" ? "telegram-bob-1026" : "telegram-group-1026";
  return {
    app: "telegram",
    account: ACCOUNT,
    kind,
    stableId: `telegram:${ACCOUNT}:${kind}:${placeId}`,
    personName: kind === "direct_message" ? "Bob" : "Bob, Chen, and Devon",
    placeName: kind === "direct_message" ? "Bob's direct message" : "Project group",
    allowed: true,
  };
}

function reader(kind: TelegramPlaceKind): TelegramPlaceReader {
  return noOpReader ? () => undefined : () => fixture(kind);
}

function reciprocal(kind: TelegramPlaceKind): TelegramVerificationState {
  const peer = kind === "direct_message" ? "telegram-bob-1026" : "telegram-group-1026";
  return {
    app: "telegram",
    kind,
    firstAccount: ACCOUNT,
    secondAccount: peer,
    firstToSecondStableId: `telegram:${ACCOUNT}:${kind}:${peer}`,
    secondToFirstStableId: `telegram:${peer}:${kind}:${ACCOUNT}`,
    firstToSecondAllowed: true,
    secondToFirstAllowed: true,
    state: "two-way",
  };
}

function inspect(kind: TelegramPlaceKind) {
  return inspectTelegramAllowedPlace(reader(kind), reciprocal(kind));
}

describe("TASK1026 Telegram direct-message and group inspection", () => {
  it("directly reads an allowed direct message and returns its allow control", () => {
    const inspected = inspect("direct_message");

    expect(inspected).not.toBeNull();
    if (!inspected) return;
    expect(inspected.kind).toBe("direct_message");
    expect(inspected.controls).toContain('data-telegram-place-kind="direct_message"');
    const controls = (inspected.controls.match(/data-telegram-whitelist-controls/gu) ?? []).length;
    expect(controls).toBe(1);
    console.log(`TASK1026 direct_message_kind=${inspected.kind} allow_controls=${controls}`);
  });

  it("directly reads an allowed group and returns its own allow control", () => {
    const inspected = inspect("group_chat");

    expect(inspected).not.toBeNull();
    if (!inspected) return;
    expect(inspected.kind).toBe("group_chat");
    expect(inspected.controls).toContain('data-telegram-place-kind="group_chat"');
    const controls = (inspected.controls.match(/data-telegram-whitelist-controls/gu) ?? []).length;
    expect(controls).toBe(1);
    console.log(`TASK1026 group_kind=${inspected.kind} allow_controls=${controls}`);
  });
});
