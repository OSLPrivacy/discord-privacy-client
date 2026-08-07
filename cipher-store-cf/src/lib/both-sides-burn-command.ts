import { parseDeleteGrantRecord } from "./delete-grant.js";

export interface AuthorizedBurnRequestInput {
  blobId: string;
  manageCap: string;
  deleteGrant: string | unknown;
}

export interface BothSidesBurnCommandInput {
  fetch: typeof fetch;
  origin: string;
  sender: AuthorizedBurnRequestInput;
  recipient: AuthorizedBurnRequestInput;
}

export interface AuthorizedBurnRequestReport {
  deleted: boolean;
  status: number;
  owner: string;
  affected_scope: string;
}

export interface BothSidesBurnCommandResult {
  sender: AuthorizedBurnRequestReport;
  recipient: AuthorizedBurnRequestReport;
}

export async function bothSidesBurnCommand(
  input: BothSidesBurnCommandInput,
): Promise<BothSidesBurnCommandResult> {
  return {
    sender: await submitAuthorizedBurnRequest(input.fetch, input.origin, "sender", input.sender),
    recipient: await submitAuthorizedBurnRequest(input.fetch, input.origin, "recipient", input.recipient),
  };
}

async function submitAuthorizedBurnRequest(
  fetchImpl: typeof fetch,
  origin: string,
  role: "sender" | "recipient",
  request: AuthorizedBurnRequestInput,
): Promise<AuthorizedBurnRequestReport> {
  const parsed = parseDeleteGrantRecord(request.deleteGrant);
  if (!parsed.ok) {
    throw new Error(`${role} delete grant refused: ${parsed.code}`);
  }

  const grantHeader = typeof request.deleteGrant === "string"
    ? request.deleteGrant
    : JSON.stringify(request.deleteGrant);
  const response = await fetchImpl(`${origin}/v1/blob/${request.blobId}`, {
    method: "DELETE",
    headers: {
      "x-osl-manage-cap": request.manageCap,
      "x-osl-delete-grant": grantHeader,
    },
  });

  return {
    deleted: response.status === 204,
    status: response.status,
    owner: parsed.grant.owner,
    affected_scope: parsed.grant.scope,
  };
}
