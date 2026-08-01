import {
  executeDeletion,
  type ExecutionRequest,
  type ProviderDeletionReceipt,
} from "./scrub-delete-engine";

/**
 * The only UI host for the deletion engine until the attended live lane is
 * explicitly enabled. Callers cannot select a live run through this surface.
 */
export type ScrubDryRunRequest = Omit<ExecutionRequest, "dryRun">;

export function executeScrubDryRun(request: ScrubDryRunRequest): Promise<ProviderDeletionReceipt> {
  return executeDeletion({ ...request, dryRun: true });
}
