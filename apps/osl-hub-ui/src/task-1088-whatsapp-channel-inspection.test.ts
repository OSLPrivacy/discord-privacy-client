import { describe, expect, it } from "vitest";
import {
  inspectWhatsAppAllowedPlace,
  type WhatsAppAllowedPlace,
  type WhatsAppPlaceReader,
} from "./whatsapp-whitelist-control";

const ACCOUNT = "whatsapp-owner-1088";
const selectedFixture = process.env.TASK1088_WHATSAPP_FIXTURE ?? "channel";

function fixture(kind: string, allowed = true): WhatsAppAllowedPlace {
  return {
    app: "whatsapp",
    account: ACCOUNT,
    kind,
    stableId: `whatsapp:${ACCOUNT}:${kind}:place-1088`,
    personName: kind === "channel" ? "WhatsApp Channel" : "Another WhatsApp place",
    placeName: `WhatsApp ${kind}`,
    allowed,
  };
}

function reader(kind: string): WhatsAppPlaceReader {
  return () => fixture(kind);
}

function inspectAllowedWhatsAppChannel(readPlace: WhatsAppPlaceReader) {
  const inspected = inspectWhatsAppAllowedPlace(readPlace);
  if (!inspected || inspected.kind !== "channel") {
    throw new Error(`TASK1088 expected allowed WhatsApp channel, received ${inspected?.kind ?? "nothing"}`);
  }
  if (!inspected.controls.includes("data-osl-whatsapp-channel-controls")) {
    throw new Error("TASK1088 allowed WhatsApp channel returned no channel controls");
  }
  return inspected;
}

describe("TASK1088 WhatsApp channel inspection", () => {
  it("directly reads an allowed channel and returns exactly channel kind and controls", () => {
    const inspected = inspectAllowedWhatsAppChannel(reader(selectedFixture));
    const controls = (inspected.controls.match(/data-osl-whatsapp-channel-controls/gu) ?? []).length;

    expect(inspected.kind).toBe("channel");
    expect(controls).toBe(1);
    expect(inspected.controls).toContain("data-osl-whatsapp-channel-toggle");
    console.log(`TASK1088_CHANNEL_KIND=${inspected.kind} TASK1088_CHANNEL_CONTROLS=${controls}`);
  });

  it("keeps the group fixture distinct and gives no other fixture channel controls", () => {
    const group = inspectWhatsAppAllowedPlace(reader("group_chat"));
    const otherFixtures = ["direct_message", "group_chat", "community", "broadcast_list"]
      .map((kind) => inspectWhatsAppAllowedPlace(reader(kind))!);
    const otherChannelCount = otherFixtures.filter(({ kind }) => kind === "channel").length;
    const otherChannelControls = otherFixtures.filter(({ controls }) => controls.includes("data-osl-whatsapp-channel-controls")).length;

    expect(group!.kind).toBe("group_chat");
    expect(group!.kind).not.toBe("channel");
    expect(group!.controls).toBe("");
    expect(otherChannelCount).toBe(0);
    expect(otherChannelControls).toBe(0);
    console.log(`TASK1088_GROUP_KIND=${group!.kind} TASK1088_OTHER_FIXTURE_CHANNEL_COUNT=${otherChannelCount} TASK1088_OTHER_FIXTURE_CHANNEL_CONTROLS=${otherChannelControls}`);
  });

  it("fails when the channel check is deliberately pointed at the group fixture", () => {
    expect(() => inspectAllowedWhatsAppChannel(reader("group_chat"))).toThrow(
      "TASK1088 expected allowed WhatsApp channel, received group_chat",
    );
    console.log("TASK1088_GROUP_AS_CHANNEL_REJECTED=true");
  });
});
