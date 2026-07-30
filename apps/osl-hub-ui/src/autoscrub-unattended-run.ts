import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "./preferences";
import {
  AUTOSCRUB_UNATTENDED_RUN_COMMAND,
  autoscrubUnattendedContractGate,
  parseAutoScrubFleetStatus,
  parseAutoScrubReviewedRunRequest,
  parseAutoscrubUnattendedContract,
  parseAutoscrubUnattendedRunStarted,
  type AutoScrubFleetStatus,
  type AutoScrubReviewedRunRequest,
  type AutoscrubUnattendedGateResult,
  type AutoscrubUnattendedRunResult,
} from "./autoscrub-contract";

export { autoscrubUnattendedContractGate };
export type { AutoscrubUnattendedGateResult, AutoscrubUnattendedRunResult };

export async function autoscrubUnattendedProductionRun(
  rawContract: unknown,
  nativeInvoke: typeof invoke = invoke,
): Promise<AutoscrubUnattendedRunResult> {
  const gate = autoscrubUnattendedContractGate(rawContract);
  if (gate.state === "refused") return gate;

  const contract = parseAutoscrubUnattendedContract(rawContract);
  if (!contract) return { state: "refused", reason: "invalid-contract" };

  const started = parseAutoscrubUnattendedRunStarted(await nativeInvoke(AUTOSCRUB_UNATTENDED_RUN_COMMAND, {
    contract,
  }).catch(() => null));
  return started ?? { state: "refused", reason: "native-refused" };
}

export async function loadAutoScrubRunFleetStatus(): Promise<AutoScrubFleetStatus | null> {
  if (!isTauriRuntime()) return null;
  return parseAutoScrubFleetStatus(await invoke<unknown>("get_autoscrub_run_fl"));
}

export async function requestAutoScrubGlobalStop(): Promise<AutoScrubFleetStatus | null> {
  if (!isTauriRuntime()) return null;
  return parseAutoScrubFleetStatus(await invoke<unknown>("request_autoscrub_global_stop"));
}

export async function startAutoScrubReviewedRun(request: AutoScrubReviewedRunRequest): Promise<AutoScrubFleetStatus | null> {
  const reviewed = parseAutoScrubReviewedRunRequest(request);
  if (!isTauriRuntime()) return null;
  return parseAutoScrubFleetStatus(await invoke<unknown>("start_autoscrub_reviewed_run", { request: reviewed }));
}
