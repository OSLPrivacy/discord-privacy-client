export interface OslHostedWorkspaceAccess {
  roomLocator: string;
  roomCapability: string;
}

function base64Url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/u, "");
}

export function createOslHostedWorkspaceAccess(): OslHostedWorkspaceAccess {
  const locator = new Uint8Array(32);
  const capability = new Uint8Array(32);
  crypto.getRandomValues(locator);
  crypto.getRandomValues(capability);
  return { roomLocator: base64Url(locator), roomCapability: base64Url(capability) };
}

export const hostedNotesAvailability = {
  available: false,
  reason: "Hosted Notes relay is not enabled in this build; local encrypted Notes are available.",
} as const;
