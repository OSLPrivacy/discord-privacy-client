import { invoke } from "@tauri-apps/api/core";
import {
  AUTOSCRUB_UNATTENDED_RUN_COMMAND,
  autoscrubUnattendedContractGate,
  parseAutoscrubUnattendedContract,
  parseAutoscrubUnattendedRunStarted,
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
