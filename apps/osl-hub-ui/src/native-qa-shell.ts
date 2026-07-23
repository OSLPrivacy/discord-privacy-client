import {
  detachNativeAppWindow,
  focusNativeAppWindow,
  hostNativeAppWindow,
  loadNativeApps,
  type NativeApp,
  type NativeAppId,
  type NativeSessionMode,
  type NativeWindowHostAction,
} from "./services";

export type NativeQaProvider = "telegram" | "signal" | "whatsapp";
export type NativeQaShellPhase = "idle" | "checking" | "opening" | "open" | "failed";
export type NativeQaShellFailure =
  | "unsupportedProvider"
  | "catalogUnavailable"
  | "appNotInstalled"
  | "busy"
  | "hostRejected"
  | "focusRejected"
  | "detachRejected"
  | "receiptMismatch";

export interface NativeQaProviderCapability {
  id: NativeQaProvider;
  displayName: string;
  sessionMode: "existingSession";
  captureProtected: false;
  credentialsAccepted: false;
  browserAutomationAllowed: false;
}

export interface NativeQaShellState {
  phase: NativeQaShellPhase;
  provider: NativeQaProvider | null;
  failure: NativeQaShellFailure | null;
  nativeReason: NativeWindowHostAction["reason"] | null;
  sessionMode: "existingSession";
  captureProtected: false;
}

export interface NativeQaShellDependencies {
  listNativeApps(): Promise<readonly NativeApp[]>;
  hostNativeAppWindow(appId: NativeAppId, mode: NativeSessionMode): Promise<NativeWindowHostAction>;
  focusNativeAppWindow(): Promise<NativeWindowHostAction>;
  detachNativeAppWindow(): Promise<NativeWindowHostAction>;
}

const capabilities: Readonly<Record<NativeQaProvider, NativeQaProviderCapability>> = Object.freeze({
  telegram: Object.freeze({
    id: "telegram",
    displayName: "Telegram",
    sessionMode: "existingSession",
    captureProtected: false,
    credentialsAccepted: false,
    browserAutomationAllowed: false,
  }),
  signal: Object.freeze({
    id: "signal",
    displayName: "Signal",
    sessionMode: "existingSession",
    captureProtected: false,
    credentialsAccepted: false,
    browserAutomationAllowed: false,
  }),
  whatsapp: Object.freeze({
    id: "whatsapp",
    displayName: "WhatsApp",
    sessionMode: "existingSession",
    captureProtected: false,
    credentialsAccepted: false,
    browserAutomationAllowed: false,
  }),
});

const defaultDependencies: NativeQaShellDependencies = {
  listNativeApps: loadNativeApps,
  hostNativeAppWindow,
  focusNativeAppWindow,
  detachNativeAppWindow,
};

const initialState = (): NativeQaShellState => ({
  phase: "idle",
  provider: null,
  failure: null,
  nativeReason: null,
  sessionMode: "existingSession",
  captureProtected: false,
});

export function nativeQaProviderCapability(provider: unknown): NativeQaProviderCapability | null {
  if (provider !== "telegram" && provider !== "signal" && provider !== "whatsapp") return null;
  return capabilities[provider];
}

/**
 * State-only orchestration for opt-in native QA shells.
 *
 * This boundary cannot install an app, collect credentials, create a profile,
 * or fall back to a browser. It opens only an already-installed app's existing
 * session and accepts only the exact borrowed-window receipt for that provider.
 */
export function createNativeQaShell(
  dependencies: NativeQaShellDependencies = defaultDependencies,
): {
  state(): NativeQaShellState;
  open(provider: unknown): Promise<NativeQaShellState>;
  focus(): Promise<NativeQaShellState>;
  close(): Promise<NativeQaShellState>;
} {
  let current = initialState();
  let operationPending = false;

  const snapshot = (): NativeQaShellState => ({ ...current });
  const transition = (next: NativeQaShellState): NativeQaShellState => {
    current = next;
    return snapshot();
  };
  const fail = (
    provider: NativeQaProvider | null,
    failure: NativeQaShellFailure,
    nativeReason: NativeWindowHostAction["reason"] | null = null,
  ): NativeQaShellState => transition({
    phase: "failed",
    provider,
    failure,
    nativeReason,
    sessionMode: "existingSession",
    captureProtected: false,
  });
  const matchesExistingSessionReceipt = (
    receipt: NativeWindowHostAction,
    provider: NativeQaProvider,
    status: "hosted" | "focused" | "detached",
  ): boolean => receipt.id === provider
    && receipt.status === status
    && receipt.reason === "none"
    && receipt.mode === "existingNativeCompanion"
    && receipt.captureProtected === false;

  return {
    state: snapshot,
    async open(provider: unknown): Promise<NativeQaShellState> {
      const capability = nativeQaProviderCapability(provider);
      if (!capability) return fail(null, "unsupportedProvider");
      if (operationPending || current.phase === "open") return fail(capability.id, "busy");

      operationPending = true;
      transition({ ...initialState(), phase: "checking", provider: capability.id });
      try {
        let catalog: readonly NativeApp[];
        try {
          catalog = await dependencies.listNativeApps();
        } catch {
          return fail(capability.id, "catalogUnavailable");
        }
        const app = catalog.find((candidate) => candidate.id === capability.id);
        if (!app || app.availability !== "installed") return fail(capability.id, "appNotInstalled");

        transition({ ...initialState(), phase: "opening", provider: capability.id });
        let receipt: NativeWindowHostAction;
        try {
          receipt = await dependencies.hostNativeAppWindow(capability.id, capability.sessionMode);
        } catch {
          return fail(capability.id, "hostRejected");
        }
        if (receipt.status !== "hosted") return fail(capability.id, "hostRejected", receipt.reason);
        if (!matchesExistingSessionReceipt(receipt, capability.id, "hosted")) {
          return fail(capability.id, "receiptMismatch", receipt.reason);
        }
        return transition({ ...initialState(), phase: "open", provider: capability.id });
      } finally {
        operationPending = false;
      }
    },
    async focus(): Promise<NativeQaShellState> {
      const provider = current.phase === "open" ? current.provider : null;
      if (!provider || operationPending) return fail(provider, "focusRejected");
      operationPending = true;
      try {
        let receipt: NativeWindowHostAction;
        try {
          receipt = await dependencies.focusNativeAppWindow();
        } catch {
          return fail(provider, "focusRejected");
        }
        if (!matchesExistingSessionReceipt(receipt, provider, "focused")) {
          return fail(provider, receipt.status === "focused" ? "receiptMismatch" : "focusRejected", receipt.reason);
        }
        return snapshot();
      } finally {
        operationPending = false;
      }
    },
    async close(): Promise<NativeQaShellState> {
      const provider = current.phase === "open" ? current.provider : null;
      if (!provider || operationPending) return fail(provider, "detachRejected");
      operationPending = true;
      try {
        let receipt: NativeWindowHostAction;
        try {
          receipt = await dependencies.detachNativeAppWindow();
        } catch {
          return fail(provider, "detachRejected");
        }
        if (!matchesExistingSessionReceipt(receipt, provider, "detached")) {
          return fail(provider, receipt.status === "detached" ? "receiptMismatch" : "detachRejected", receipt.reason);
        }
        return transition(initialState());
      } finally {
        operationPending = false;
      }
    },
  };
}
