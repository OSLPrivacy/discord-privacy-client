export interface DiscordOverlayReceivePollingState {
  overlayReady: boolean;
  decryptDisplayEnabled: boolean;
  documentHidden: boolean;
  discordQaShell: boolean;
}

export function shouldPollDiscordOverlay(
  state: DiscordOverlayReceivePollingState,
): boolean {
  return state.overlayReady
    && state.decryptDisplayEnabled
    && (state.discordQaShell || !state.documentHidden);
}
