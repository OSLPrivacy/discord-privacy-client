/**
 * Task 0213: the add-friend name request box.
 *
 * A small self-contained panel: one OSL-name input, one Send Request action,
 * and a pending-result area that always states how many requests are pending.
 * Submissions go through `osl_create_friend_request_by_osl_name` (task 0211).
 * That command refuses a name the directory does not know with
 * "OSL: unknown OSL name" (task 0212); this box surfaces that refusal naming
 * the exact name it refused, and adds nothing to the pending list.
 */

export interface PendingNameRequest {
  readonly requestId: string;
  readonly recipientName: string;
}

export type AddFriendNameRequestStatus =
  | { readonly kind: "idle" }
  | { readonly kind: "sent"; readonly recipientName: string }
  | { readonly kind: "refused"; readonly recipientName: string; readonly detail: string }
  | { readonly kind: "failed"; readonly detail: string };

export interface AddFriendNameRequestModel {
  readonly pending: readonly PendingNameRequest[];
  readonly status: AddFriendNameRequestStatus;
}

export function blankAddFriendNameRequestModel(): AddFriendNameRequestModel {
  return { pending: [], status: { kind: "idle" } };
}

export interface AddFriendNameRequestBackend {
  invoke(
    command: "osl_create_friend_request_by_osl_name",
    args: { recipientName: string; requestId: string },
  ): Promise<unknown>;
  newRequestId(): string;
}

/**
 * The backend refuses a name that resolves to nobody with
 * "OSL: unknown OSL name" (crates/ipc, task 0212). Match on the phrase, not
 * the full string, so an added prefix or suffix does not turn a refusal into
 * a generic failure.
 */
function isUnknownNameRefusal(detail: string): boolean {
  return detail.includes("unknown OSL name");
}

/**
 * Submit one name. Returns the next model; never throws. A refused or failed
 * submission returns the pending list unchanged — the only transition that
 * grows it is a backend acceptance, so the pending count can never drift from
 * what the backend actually created.
 */
export async function submitAddFriendNameRequest(
  model: AddFriendNameRequestModel,
  rawName: string,
  backend: AddFriendNameRequestBackend,
): Promise<AddFriendNameRequestModel> {
  const recipientName = rawName.trim();
  if (!recipientName) {
    return {
      ...model,
      status: { kind: "failed", detail: "Enter an OSL name before sending a request." },
    };
  }
  const requestId = backend.newRequestId();
  try {
    await backend.invoke("osl_create_friend_request_by_osl_name", { recipientName, requestId });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    if (isUnknownNameRefusal(detail)) {
      return { ...model, status: { kind: "refused", recipientName, detail } };
    }
    return { ...model, status: { kind: "failed", detail } };
  }
  return {
    pending: [...model.pending, { requestId, recipientName }],
    status: { kind: "sent", recipientName },
  };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;")
    .replace(/'/gu, "&#39;");
}

function statusMarkup(status: AddFriendNameRequestStatus): string {
  switch (status.kind) {
    case "idle":
      return "Requests you send appear below and stay pending until the other person accepts.";
    case "sent":
      return `Request sent to ${escapeHtml(status.recipientName)}. It is pending until they accept.`;
    case "refused":
      return `${escapeHtml(status.recipientName)} is not a known OSL name. No request was created.`;
    case "failed":
      return escapeHtml(status.detail);
  }
}

export function addFriendNameRequestMarkup(model: AddFriendNameRequestModel): string {
  const count = model.pending.length;
  const entries = count
    ? `<ul class="pending-name-request-list">${model.pending
        .map(
          (request) =>
            `<li data-pending-name-request="${escapeHtml(request.recipientName)}">Request to ${escapeHtml(request.recipientName)} is pending until they accept.</li>`,
        )
        .join("")}</ul>`
    : `<p class="empty-state">No pending name requests.</p>`;
  return `<section class="add-friend-name-request" aria-labelledby="add-friend-name-request-title"><h2 id="add-friend-name-request-title">Add a friend by OSL name</h2><label for="add-friend-name-input"><span>Their exact OSL name</span><input id="add-friend-name-input" data-add-friend-name placeholder="OSL name" autocomplete="off" autocapitalize="none" spellcheck="false"/></label><button class="button primary" data-send-name-request type="button">Send Request</button><p class="form-status" data-name-request-status role="status">${statusMarkup(model.status)}</p><div class="pending-name-request-result" aria-label="Pending name requests"><strong data-pending-count="${count}">Pending requests (${count})</strong>${entries}</div></section>`;
}
