export type DeviceTransferManifestSection = Readonly<{
  heading: string;
  items: readonly string[];
}>;

export type DeviceTransferManifestScreen = Readonly<{
  title: string;
  sections: readonly DeviceTransferManifestSection[];
}>;

/**
 * The transfer UI must make the boundary explicit before a user starts a
 * transfer. This is presentation data so the host screen can render it without
 * inventing or omitting categories.
 */
export function deviceTransferManifestScreen(): DeviceTransferManifestScreen {
  return {
    title: "Move your OSL account to another device",
    sections: [
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
    ],
  };
}
