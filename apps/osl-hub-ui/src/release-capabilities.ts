/**
 * TASK 6100 — publish the accepted shard-six release limits in exact words.
 *
 * The values here are frozen by TASK 6101. They are not inferred from runtime
 * state and must not be narrowed or widened by this build. The UI surfaces that
 * render these words import from this module so the same authoritative values
 * appear everywhere.
 */

export type ContentTypeId = "text" | "attachment" | "paste" | "share" | "streaming";
export type IntegrationId = "discord" | "telegram" | "signal" | "whatsapp" | "x" | "instagram" | "messenger";
export type SupportStatus = "Supported" | "Unsupported";

export interface ContentCell {
  readonly integration: IntegrationId;
  readonly integrationLabel: string;
  readonly contentType: ContentTypeId;
  readonly contentTypeLabel: string;
  readonly status: SupportStatus;
}

/** The exact seven integrations in the 6101 frozen matrix, row for row. */
export const RELEASE_INTEGRATIONS: readonly { readonly id: IntegrationId; readonly label: string }[] = Object.freeze([
  { id: "discord", label: "Discord" },
  { id: "telegram", label: "Telegram" },
  { id: "signal", label: "Signal" },
  { id: "whatsapp", label: "WhatsApp" },
  { id: "x", label: "X" },
  { id: "instagram", label: "Instagram" },
  { id: "messenger", label: "Messenger" },
]);

/** The exact five content types in the 6101 frozen matrix, column for column. */
export const RELEASE_CONTENT_TYPES: readonly { readonly id: ContentTypeId; readonly label: string }[] = Object.freeze([
  { id: "text", label: "Text" },
  { id: "attachment", label: "Attachment" },
  { id: "paste", label: "Paste" },
  { id: "share", label: "Share" },
  { id: "streaming", label: "Streaming" },
]);

/** The 6101 frozen shipping implementation id. */
export const SHIPPING_IMPLEMENTATION_ID = "native.discord.text.v1";

/** The 6101 frozen supported cell. */
export const SHIPPING_CONTENT_TYPE: ContentTypeId = "text";

/** The 6101 frozen carrier list, in display order. */
export const SHIPPING_CARRIERS: readonly string[] = Object.freeze(["Discord"]);

/** The 6101 frozen Strip adapter list, in display order. */
export const SHIPPING_STRIP_ADAPTERS: readonly string[] = Object.freeze(["Discord"]);

/** The 6101 contract-only unavailable adapters. */
export const CONTRACT_ONLY_INTEGRATIONS: readonly IntegrationId[] = Object.freeze(["x", "instagram", "messenger"]);

function cellStatus(integration: IntegrationId, contentType: ContentTypeId): SupportStatus {
  return integration === "discord" && contentType === "text" ? "Supported" : "Unsupported";
}

/** The complete 35-row integration-by-content-type matrix, row for row. */
export function releaseCapabilityMatrix(): readonly ContentCell[] {
  const rows: ContentCell[] = [];
  for (const integration of RELEASE_INTEGRATIONS) {
    for (const contentType of RELEASE_CONTENT_TYPES) {
      rows.push({
        integration: integration.id,
        integrationLabel: integration.label,
        contentType: contentType.id,
        contentTypeLabel: contentType.label,
        status: cellStatus(integration.id, contentType.id),
      });
    }
  }
  return Object.freeze(rows);
}

function escapeHtml(value: string): string {
  return value.replace(/[&<>"']/gu, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character]!);
}

/**
 * Exact catalogue-backed sentences for TASK 6100 surfaces.
 *
 * These are rendered through the shipping English catalogue (crates/english-catalogue)
 * and the in-module helper so placeholders are resolved to the frozen 6101 values
 * before any surface is shown.
 */
export const RELEASE_CAPABILITY_COPY = Object.freeze({
  /** "This release can send through: Discord. All other integrations are unavailable..." */
  carrierListSentence: `This release can send through: ${SHIPPING_CARRIERS.join(", ")}. All other integrations are unavailable and are not covered by send, offline, Tor or protection tests.`,

  /** "For each integration, only the content types marked Supported are covered..." */
  matrixExplanationSentence:
    "For each integration, only the content types marked Supported are covered by send, offline, Tor, and protection tests; text support does not imply attachment, paste, share, or streaming support.",

  /** "Quick settings are verified only for: Discord. On every other integration..." */
  stripHelpSentence: `Quick settings are verified only for: ${SHIPPING_STRIP_ADAPTERS.join(", ")}. On every other integration, use full Settings; Strip controls may be unavailable.`,

  /** "Notification actions are informational shortcuts; confirm security..." */
  notificationHelpSentence:
    "Notification actions are informational shortcuts; confirm security, recovery and payment state inside the app before acting.",

  /** "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL." */
  independentExportSentence:
    "Exports are independent copies. Burn, timers, retention, disconnect, and account deletion cannot remove an archive or key you saved outside OSL.",

  /** "Storing the archive and its key together defeats the encryption..." */
  exportKeyStorageSentence:
    "Storing the archive and its key together defeats the encryption. Keep the key in a different protected location; anyone who obtains both can read the export.",

  /** "After {period} without a successfully authenticated foreground owner action..." */
  successionSentence: (period: string, successor: string) =>
    `After ${period} without a successfully authenticated foreground owner action, ownership transfers automatically to ${successor} and you may lose owner access. Background sync does not reset this timer.`,

  /** "Anyone who obtains this recovery kit may race to take over the account..." */
  recoveryKitTheftSentence:
    "Anyone who obtains this recovery kit may race to take over the account and revoke your devices. Store it encrypted and offline.",
});

/** Renders the release-capabilities surface with the frozen carrier list and matrix. */
export function releaseCapabilitiesMarkup(): string {
  const matrix = releaseCapabilityMatrix();
  const headers = ["", ...RELEASE_CONTENT_TYPES.map((type) => type.label)];
  const headerRow = `<thead><tr>${headers.map((header) => `<th>${escapeHtml(header)}</th>`).join("")}</tr></thead>`;
  const bodyRows = RELEASE_INTEGRATIONS.map((integration) => {
    const cells = RELEASE_CONTENT_TYPES.map((contentType) => {
      const cell = matrix.find((candidate) => candidate.integration === integration.id && candidate.contentType === contentType.id)!;
      return `<td data-integration="${integration.id}" data-content-type="${contentType.id}" data-status="${cell.status}">${escapeHtml(cell.status)}</td>`;
    }).join("");
    return `<tr data-integration="${integration.id}"><th scope="row">${escapeHtml(integration.label)}</th>${cells}</tr>`;
  }).join("");
  const matrixTable = `<table class="release-capability-matrix" aria-label="Release capability matrix">${headerRow}<tbody>${bodyRows}</tbody></table>`;
  return [
    `<section class="release-capabilities" aria-labelledby="release-capabilities-title">`,
    `<header><h1 id="release-capabilities-title">Release capabilities</h1><p>${escapeHtml(RELEASE_CAPABILITY_COPY.carrierListSentence)}</p></header>`,
    matrixTable,
    `<p class="release-capability-matrix-note">${escapeHtml(RELEASE_CAPABILITY_COPY.matrixExplanationSentence)}</p>`,
    `</section>`,
  ].join("");
}

/** The exact carrier list rendered by 6101 row for row. */
export function shippingCarrierList(): readonly string[] {
  return SHIPPING_CARRIERS;
}

/** The exact Strip adapter list rendered by 6101 row for row. */
export function shippingStripAdapterList(): readonly string[] {
  return SHIPPING_STRIP_ADAPTERS;
}
