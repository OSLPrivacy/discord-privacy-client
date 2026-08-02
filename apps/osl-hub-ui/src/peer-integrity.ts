import { escapeHtml } from "./services";

/** A peer-reported capture-protection state; absence is deliberately distinct. */
export type PeerIntegrityState = "matching" | "unpublished" | "unknown";

export function peerIntegrityMarkup(state: PeerIntegrityState): string {
  const content: Record<PeerIntegrityState, { label: string; detail: string }> = {
    matching: {
      label: "Reported capture protection",
      detail: "This person's client reported screenshot protection matching this chat's requirement. This is a report from their client, not proof that their device cannot capture content.",
    },
    unpublished: {
      label: "Reported, not published",
      detail: "This person's client reported screenshot protection, but it has not published a matching confirmation for this chat.",
    },
    unknown: {
      label: "Not reported",
      detail: "This person's client has not confirmed screenshot protection.",
    },
  };
  const item = content[state];
  return `<div class="peer-integrity peer-integrity--${state}" data-peer-integrity="${state}"><strong>${escapeHtml(item.label)}</strong><small>${escapeHtml(item.detail)}</small></div>`;
}
