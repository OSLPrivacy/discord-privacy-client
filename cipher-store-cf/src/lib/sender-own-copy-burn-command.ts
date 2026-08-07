import { parseDeleteGrantRecord } from "./delete-grant.js";

export interface SenderOwnCopyBurnCommandInput {
  fetch: typeof fetch;
  origin: string;
  blobId: string;
  manageCap: string;
  senderDeleteGrant: string | unknown;
}

export interface SenderOwnCopyBurnCommandResult {
  deleted: boolean;
  status: number;
  affected_scope: string;
}

export async function senderOwnCopyBurnCommand(
  input: SenderOwnCopyBurnCommandInput,
): Promise<SenderOwnCopyBurnCommandResult> {
  const parsed = parseDeleteGrantRecord(input.senderDeleteGrant);
  if (!parsed.ok) {
    throw new Error(`sender delete grant refused: ${parsed.code}`);
  }

  const grantHeader = typeof input.senderDeleteGrant === "string"
    ? input.senderDeleteGrant
    : JSON.stringify(input.senderDeleteGrant);
  const response = await input.fetch(`${input.origin}/v1/blob/${input.blobId}`, {
    method: "DELETE",
    headers: {
      "x-osl-manage-cap": input.manageCap,
      "x-osl-delete-grant": grantHeader,
    },
  });

  return {
    deleted: response.status === 204,
    status: response.status,
    affected_scope: parsed.grant.scope,
  };
}
