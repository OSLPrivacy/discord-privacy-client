/**
 * Inputs are facts supplied by the Enclave transport and roster layers.  This
 * component deliberately does not infer a successful delivery from a queued
 * operation or a missing acknowledgement.
 */
export type OslEnclaveState = {
  offline?: boolean;
  queuedSends?: number;
  staleRoster?: boolean;
  burnRequestsQueued?: number;
  removalUnconfirmed?: boolean;
  acknowledgementsOutstanding?: number;
};

function countLine(count: number, singular: string, plural: string): string {
  return count === 1 ? singular : plural.replace("{count}", String(count));
}

/** Renders the state region shared by the Enclave surface. */
export function oslEnclaveStateMarkup(state: OslEnclaveState): string {
  const notices: string[] = [];
  const queuedSends = Math.max(0, state.queuedSends ?? 0);
  const burnRequestsQueued = Math.max(0, state.burnRequestsQueued ?? 0);
  const acknowledgementsOutstanding = Math.max(0, state.acknowledgementsOutstanding ?? 0);

  if (state.offline) {
    const queued = queuedSends > 0
      ? ` ${countLine(queuedSends, "1 message is waiting to send when you're back online.", "{count} messages are waiting to send when you're back online.")}`
      : "";
    notices.push(`<section class="osl-enclave-state osl-enclave-state--offline" role="status"><strong>You're offline</strong><p>Sending and receiving need a network connection.${queued}</p></section>`);
  }

  if (state.staleRoster) {
    notices.push('<section class="osl-enclave-state osl-enclave-state--stale-roster" role="status"><strong>Membership needs updating</strong><p>Posting is unavailable until membership is current.</p></section>');
  }

  if (burnRequestsQueued > 0) {
    notices.push(`<section class="osl-enclave-state osl-enclave-state--burn-queued" role="status"><strong>Removal request queued</strong><p>${countLine(burnRequestsQueued, "1 removal request is queued for the server.", "{count} removal requests are queued for the server.")}</p></section>`);
  }

  if (state.removalUnconfirmed) {
    notices.push('<section class="osl-enclave-state osl-enclave-state--removal-unconfirmed" role="status"><strong>Removal is not fully verifiable</strong><p>A removed member may retain content they already have.</p></section>');
  }

  if (acknowledgementsOutstanding > 0) {
    notices.push(`<section class="osl-enclave-state osl-enclave-state--ack-unconfirmed" role="status"><strong>Acknowledgement outstanding</strong><p>${countLine(acknowledgementsOutstanding, "1 acknowledgement is still outstanding.", "{count} acknowledgements are still outstanding.")}</p></section>`);
  }

  return notices.length ? `<aside class="osl-enclave-states" aria-label="Enclave status">${notices.join("")}</aside>` : "";
}
