/** Shipping English catalogue entries used by release-limit and account journeys. */
export const englishCatalogue = Object.freeze({
  accountExportKeyWarning: "OSL cannot recover your export key. Save and verify it before leaving this screen.",
  accountExportIndependentCopyWarning: "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL.",
  accountExportKeyStorageWarning: "Storing the archive and its key together defeats the encryption. Keep the key in a different protected location; anyone who obtains both can read the export.",
  releaseCapabilityCarrierList: "This release can send through: Discord. All other integrations are unavailable and are not covered by send, offline, Tor or protection tests.",
  releaseCapabilityMatrixNote: "For each integration, only the content types marked Supported are covered by send, offline, Tor, and protection tests; text support does not imply attachment, paste, share, or streaming support.",
  stripHelp: "Quick settings are verified only for: Discord. On every other integration, use full Settings; Strip controls may be unavailable.",
  notificationHelp: "Notification actions are informational shortcuts; confirm security, recovery and payment state inside the app before acting.",
  recoveryKitTheftWarning: "Anyone who obtains this recovery kit may race to take over the account and revoke your devices. Store it encrypted and offline.",
  senderDraftFilterAdvisory: "Advisory: this is your device checking your own draft. A modified client would not run this check, and nothing prevents the message from arriving.",
  successionWarning: (period: string, successor: string) => `After ${period} without a successfully authenticated foreground owner action, ownership transfers automatically to ${successor} and you may lose owner access. Background sync does not reset this timer.`,
});
