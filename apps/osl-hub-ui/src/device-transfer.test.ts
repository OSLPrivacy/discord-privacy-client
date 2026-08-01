import { describe, expect, it } from "vitest";
import { deviceTransferManifestScreen } from "./device-transfer";

describe("T15-T31 device-transfer manifest", () => {
  it("names every category that moves and stays on the source device", () => {
    const screen = deviceTransferManifestScreen();

    expect(screen.title).toBe("Move your OSL account to another device");
    expect(screen.sections).toEqual([
      {
        heading: "Moves with your account",
        items: [
          "Identity",
          "Message history",
          "Peer map",
          "Verification flags",
          "Whitelist",
          "Burn list",
        ],
      },
      {
        heading: "Stays on this device",
        items: [
          "Per-device keys",
          "Pending inbound messages already fetched by this device",
          "Operating-system credential-store material",
        ],
      },
    ]);
  });
});
