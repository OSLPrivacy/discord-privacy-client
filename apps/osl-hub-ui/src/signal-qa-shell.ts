import type { NativeApp } from "./services";

export type SignalQaShellPhase = "idle" | "checking" | "opening" | "open" | "failed";
export type SignalQaShellFailure =
  | "catalogUnavailable"
  | "appNotInstalled"
  | "busy"
  | "hostRejected"
  | "focusRejected"
  | "detachRejected"
  | "receiptMismatch";

export type SignalQaNativeReason =
  | "none"
  | "platformUnsupported"
  | "secondaryInstanceUnverified"
  | "existingSessionNotFound"
  | "existingSessionAmbiguous"
  | "appNotInstalled"
  | "launchFailed"
  | "windowNotFound"
  | "windowIdentityChanged"
  | "ownerWindowUnavailable"
  | "hostWindowUnavailable"
  | "windowOperationRejected"
  | "notHosted";

export interface SignalQaCapability {
  provider: "signal";
  displayName: "Signal";
  desktopOnly: true;
  sessionMode: "existingSession";
  captureProtected: false;
  credentialsAccepted: false;
  browserAutomationAllowed: false;
}

export interface SignalQaShellState {
  phase: SignalQaShellPhase;
  provider: "signal";
  failure: SignalQaShellFailure | null;
  nativeReason: SignalQaNativeReason | null;
  sessionMode: "existingSession";
  captureProtected: false;
}

export interface SignalQaNativeReceipt {
  id: "signal";
  status: "existingSession" | "focused" | "detached" | "failed";
  reason: SignalQaNativeReason;
  mode: "existingNativeCompanion" | "none" | "ownedBorderless";
  captureProtected: boolean;
}

export interface SignalQaShellDependencies {
  listNativeApps(): Promise<readonly NativeApp[]>;
  hostNativeAppWindow(appId: "signal", mode: "existingSession"): Promise<SignalQaNativeReceipt>;
  focusNativeAppWindow(): Promise<SignalQaNativeReceipt>;
  detachNativeAppWindow(): Promise<SignalQaNativeReceipt>;
}

export type SignalQaCheckStatus = "notRun" | "running" | "passed" | "failed" | "blocked" | "expired";

export type SignalQaTestId =
  | "text"
  | "multilineUtf8"
  | "encryption"
  | "transcriptOverlay"
  | "burn"
  | "covertext"
  | "attachmentsImages"
  | "receipts"
  | "reconnect"
  | "replayRejection"
  | "malformedRejection"
  | "expiry"
  | "windowLifecycle";

export type SignalBindingStatus = "accepted" | "rejected" | "invalidated" | "authorized";
export type SignalBindingReason =
  | "none"
  | "noClaimedWindow"
  | "malformedEvidence"
  | "windowGenerationChanged"
  | "windowIdentityChanged"
  | "windowUnavailable"
  | "windowNotForeground"
  | "composerNotFocused"
  | "conversationUnstable"
  | "conversationChanged"
  | "composerChanged"
  | "staleEvidence"
  | "replayedEvidence"
  | "replayJournalFull"
  | "noFreshAttestation"
  | "attestationExpired"
  | "attestationSuperseded"
  | "destinationMismatch"
  | "alreadyConsumed"
  | "stateUnavailable";

/** Exact safe IPC shape returned by get_signal_protected_send_readiness. */
export interface SignalBindingReceipt {
  status: SignalBindingStatus;
  reason: SignalBindingReason;
  lifecycleGeneration: number;
  attestationGeneration: number;
  validForMs: number;
}

export interface SignalQaSemanticReceipt {
  windowClaim: SignalQaCheckStatus;
  destination: SignalQaCheckStatus;
  composer: SignalQaCheckStatus;
  freshness: SignalQaCheckStatus;
  protectedComposer: "available" | "blocked";
  bindingStatus: SignalBindingStatus | null;
  bindingReason: SignalBindingReason | null;
  lifecycleGeneration: number | null;
  attestationGeneration: number | null;
  validForMs: number;
  tests: Readonly<Record<SignalQaTestId, SignalQaCheckStatus>>;
}

export const signalQaTestDefinitions: readonly Readonly<{ id: SignalQaTestId; label: string }>[] = Object.freeze([
  Object.freeze({ id: "text", label: "Text" }),
  Object.freeze({ id: "multilineUtf8", label: "Multiline / UTF-8" }),
  Object.freeze({ id: "encryption", label: "Encryption" }),
  Object.freeze({ id: "transcriptOverlay", label: "Transcript overlay" }),
  Object.freeze({ id: "burn", label: "Burn" }),
  Object.freeze({ id: "covertext", label: "Covertext" }),
  Object.freeze({ id: "attachmentsImages", label: "Attachments / images" }),
  Object.freeze({ id: "receipts", label: "Delivery / read receipts" }),
  Object.freeze({ id: "reconnect", label: "Reconnect" }),
  Object.freeze({ id: "replayRejection", label: "Replay rejection" }),
  Object.freeze({ id: "malformedRejection", label: "Malformed-data rejection" }),
  Object.freeze({ id: "expiry", label: "Expiry" }),
  Object.freeze({ id: "windowLifecycle", label: "Window lifecycle" }),
]);

const bindingStatuses = new Set<SignalBindingStatus>(["accepted", "rejected", "invalidated", "authorized"]);
const bindingReasons = new Set<SignalBindingReason>([
  "none", "noClaimedWindow", "malformedEvidence", "windowGenerationChanged", "windowIdentityChanged",
  "windowUnavailable", "windowNotForeground", "composerNotFocused", "conversationUnstable",
  "conversationChanged", "composerChanged", "staleEvidence", "replayedEvidence", "replayJournalFull",
  "noFreshAttestation", "attestationExpired", "attestationSuperseded", "destinationMismatch", "alreadyConsumed",
  "stateUnavailable",
]);
const maxAttestationLifetimeMs = 5_000;

function emptyTestStatuses(status: SignalQaCheckStatus): Record<SignalQaTestId, SignalQaCheckStatus> {
  return Object.fromEntries(signalQaTestDefinitions.map(({ id }) => [id, status])) as Record<SignalQaTestId, SignalQaCheckStatus>;
}

export function parseSignalBindingReceipt(raw: unknown): SignalBindingReceipt | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) return null;
  const candidate = raw as Record<string, unknown>;
  if (typeof candidate.status !== "string" || !bindingStatuses.has(candidate.status as SignalBindingStatus)) return null;
  if (typeof candidate.reason !== "string" || !bindingReasons.has(candidate.reason as SignalBindingReason)) return null;
  const lifecycleGeneration = candidate.lifecycleGeneration;
  const attestationGeneration = candidate.attestationGeneration;
  const validForMs = candidate.validForMs;
  if (!Number.isSafeInteger(lifecycleGeneration) || Number(lifecycleGeneration) < 0
    || !Number.isSafeInteger(attestationGeneration) || Number(attestationGeneration) < 0
    || !Number.isSafeInteger(validForMs) || Number(validForMs) < 0 || Number(validForMs) > maxAttestationLifetimeMs) return null;
  return {
    status: candidate.status as SignalBindingStatus,
    reason: candidate.reason as SignalBindingReason,
    lifecycleGeneration: Number(lifecycleGeneration),
    attestationGeneration: Number(attestationGeneration),
    validForMs: Number(validForMs),
  };
}

/**
 * Converts an untrusted backend receipt to fixed semantic statuses. No provider
 * labels, account identifiers, participant data, composer text, or message
 * contents are copied into the result.
 */
export function signalQaSemanticReceipt(
  raw: unknown,
  nativeWindowClaimed: boolean,
  elapsedSinceReceiptMs: number = 0,
): SignalQaSemanticReceipt {
  const blocked = (): SignalQaSemanticReceipt => ({
    windowClaim: nativeWindowClaimed ? "passed" : "blocked",
    destination: "blocked",
    composer: "blocked",
    freshness: "blocked",
    protectedComposer: "blocked",
    bindingStatus: null,
    bindingReason: null,
    lifecycleGeneration: null,
    attestationGeneration: null,
    validForMs: 0,
    tests: Object.freeze(emptyTestStatuses("blocked")),
  });
  const candidate = parseSignalBindingReceipt(raw);
  if (!candidate) return blocked();
  const accepted = candidate.status === "accepted"
    && candidate.reason === "none"
    && candidate.lifecycleGeneration > 0
    && candidate.attestationGeneration > 0
    && candidate.validForMs > 0;
  const fresh = accepted && Number.isFinite(elapsedSinceReceiptMs)
    && elapsedSinceReceiptMs >= 0
    && elapsedSinceReceiptMs < candidate.validForMs;
  const exactWindow = nativeWindowClaimed;
  const tests = emptyTestStatuses(accepted ? "notRun" : "blocked");
  return {
    windowClaim: exactWindow ? "passed" : "blocked",
    destination: exactWindow && accepted ? "passed" : "blocked",
    composer: exactWindow && accepted ? "passed" : "blocked",
    freshness: fresh ? "passed" : accepted ? "expired" : "blocked",
    protectedComposer: exactWindow && fresh ? "available" : "blocked",
    bindingStatus: candidate.status,
    bindingReason: candidate.reason,
    lifecycleGeneration: candidate.lifecycleGeneration,
    attestationGeneration: candidate.attestationGeneration,
    validForMs: fresh ? Math.max(0, candidate.validForMs - elapsedSinceReceiptMs) : 0,
    tests: Object.freeze(tests),
  };
}

export interface SignalQaAttestationDependencies {
  getSignalProtectedSendReadiness(): Promise<unknown>;
  nowMs(): number;
}

export function createSignalQaAttestationView(dependencies: SignalQaAttestationDependencies): {
  state(nativeWindowClaimed: boolean): SignalQaSemanticReceipt;
  refresh(nativeWindowClaimed: boolean): Promise<SignalQaSemanticReceipt>;
  clear(): void;
} {
  let receipt: SignalBindingReceipt | null = null;
  let receivedAtMs = 0;
  const state = (nativeWindowClaimed: boolean): SignalQaSemanticReceipt => signalQaSemanticReceipt(
    receipt,
    nativeWindowClaimed,
    receipt ? dependencies.nowMs() - receivedAtMs : 0,
  );
  return {
    state,
    async refresh(nativeWindowClaimed: boolean): Promise<SignalQaSemanticReceipt> {
      try {
        receipt = parseSignalBindingReceipt(await dependencies.getSignalProtectedSendReadiness());
        receivedAtMs = dependencies.nowMs();
      } catch {
        receipt = null;
        receivedAtMs = 0;
      }
      return state(nativeWindowClaimed);
    },
    clear(): void {
      receipt = null;
      receivedAtMs = 0;
    },
  };
}

export const signalQaCapability: SignalQaCapability = Object.freeze({
  provider: "signal",
  displayName: "Signal",
  desktopOnly: true,
  sessionMode: "existingSession",
  captureProtected: false,
  credentialsAccepted: false,
  browserAutomationAllowed: false,
});

const initialState = (): SignalQaShellState => ({
  phase: "idle",
  provider: "signal",
  failure: null,
  nativeReason: null,
  sessionMode: "existingSession",
  captureProtected: false,
});

/**
 * State-only orchestration for the opt-in Signal Desktop QA shell.
 *
 * This boundary cannot install Signal, collect credentials, create or replace
 * a profile, or fall back to a browser. It can claim only the already-installed
 * Signal Desktop window for the user's existing linked session.
 */
export function createSignalQaShell(dependencies: SignalQaShellDependencies): {
  state(): SignalQaShellState;
  open(): Promise<SignalQaShellState>;
  focus(): Promise<SignalQaShellState>;
  close(): Promise<SignalQaShellState>;
} {
  let current = initialState();
  let operationPending = false;

  const snapshot = (): SignalQaShellState => ({ ...current });
  const transition = (next: SignalQaShellState): SignalQaShellState => {
    current = next;
    return snapshot();
  };
  const fail = (
    failure: SignalQaShellFailure,
    nativeReason: SignalQaNativeReason | null = null,
  ): SignalQaShellState => transition({
    ...initialState(),
    phase: "failed",
    failure,
    nativeReason,
  });
  const matchesReceipt = (
    receipt: SignalQaNativeReceipt,
    status: "existingSession" | "focused" | "detached",
  ): boolean => receipt.id === "signal"
    && receipt.status === status
    && receipt.reason === "none"
    && receipt.mode === "existingNativeCompanion"
    && receipt.captureProtected === false;

  return {
    state: snapshot,
    async open(): Promise<SignalQaShellState> {
      if (operationPending || current.phase === "open") return fail("busy");

      operationPending = true;
      transition({ ...initialState(), phase: "checking" });
      try {
        let catalog: readonly NativeApp[];
        try {
          catalog = await dependencies.listNativeApps();
        } catch {
          return fail("catalogUnavailable");
        }
        const signal = catalog.find((candidate) => candidate.id === "signal");
        if (!signal || signal.availability !== "installed") return fail("appNotInstalled");

        transition({ ...initialState(), phase: "opening" });
        let receipt: SignalQaNativeReceipt;
        try {
          receipt = await dependencies.hostNativeAppWindow("signal", "existingSession");
        } catch {
          return fail("hostRejected");
        }
        if (receipt.status !== "existingSession") return fail("hostRejected", receipt.reason);
        if (!matchesReceipt(receipt, "existingSession")) return fail("receiptMismatch", receipt.reason);
        return transition({ ...initialState(), phase: "open" });
      } finally {
        operationPending = false;
      }
    },
    async focus(): Promise<SignalQaShellState> {
      if (current.phase !== "open" || operationPending) return fail("focusRejected");
      operationPending = true;
      try {
        let receipt: SignalQaNativeReceipt;
        try {
          receipt = await dependencies.focusNativeAppWindow();
        } catch {
          return fail("focusRejected");
        }
        if (!matchesReceipt(receipt, "focused")) {
          return fail(receipt.status === "focused" ? "receiptMismatch" : "focusRejected", receipt.reason);
        }
        return snapshot();
      } finally {
        operationPending = false;
      }
    },
    async close(): Promise<SignalQaShellState> {
      if (current.phase !== "open" || operationPending) return fail("detachRejected");
      operationPending = true;
      try {
        let receipt: SignalQaNativeReceipt;
        try {
          receipt = await dependencies.detachNativeAppWindow();
        } catch {
          return fail("detachRejected");
        }
        if (!matchesReceipt(receipt, "detached")) {
          return fail(receipt.status === "detached" ? "receiptMismatch" : "detachRejected", receipt.reason);
        }
        return transition(initialState());
      } finally {
        operationPending = false;
      }
    },
  };
}
