import {
  detachDefaultBrowserCompanion,
  focusDefaultBrowserCompanion,
  hostBrowserCompanion,
  loadDefaultBrowserCompanionStatus,
  resizeDefaultBrowserCompanion,
  type BrowserCompanionAction,
  type BrowserCompanionStatus,
  type HomeAppId,
} from "./services";

/**
 * Browser surfaces covered by the QA shell. Native messengers and Outlook are
 * deliberately absent: callers cannot turn this into a general URL launcher.
 */
export const browserServiceQaIds = [
  "instagram",
  "snapchat",
  "x",
  "messenger",
  "gmail",
  "proton",
  "yahoo",
  "aol",
  "gmx",
  "maildotcom",
  "icloud",
] as const satisfies readonly HomeAppId[];

export type BrowserServiceQaId = (typeof browserServiceQaIds)[number];
export type BrowserServiceQaPhase = "idle" | "checking" | "opening" | "hosted" | "failed";
export type BrowserServiceQaError =
  | "unsupportedService"
  | "defaultFirefoxUnavailable"
  | "operationInProgress"
  | "hostRejected"
  | "focusRejected"
  | "resizeRejected"
  | "detachRejected";

export interface BrowserServiceQaState {
  readonly phase: BrowserServiceQaPhase;
  readonly serviceId: BrowserServiceQaId | null;
  readonly browserId: "firefox" | null;
  readonly sessionOwnership: "browser";
  readonly credentialAccess: "none";
  readonly captureProtected: false;
  readonly containment: "bestEffort";
  readonly error: BrowserServiceQaError | null;
  readonly revision: number;
}

export interface BrowserServiceQaPort {
  inspectDefaultBrowser(): Promise<BrowserCompanionStatus>;
  hostExistingFirefoxSession(serviceId: BrowserServiceQaId): Promise<BrowserCompanionAction>;
  focus(): Promise<BrowserCompanionAction>;
  resize(): Promise<BrowserCompanionAction>;
  detach(): Promise<BrowserCompanionAction>;
}

export interface BrowserServiceQaShell {
  snapshot(): BrowserServiceQaState;
  open(serviceId: BrowserServiceQaId): Promise<BrowserServiceQaState>;
  focus(): Promise<BrowserServiceQaState>;
  resize(): Promise<BrowserServiceQaState>;
  detach(): Promise<BrowserServiceQaState>;
  subscribe(listener: (state: BrowserServiceQaState) => void): () => void;
}

const supportedServices = new Set<string>(browserServiceQaIds);

const productionPort: BrowserServiceQaPort = {
  inspectDefaultBrowser: loadDefaultBrowserCompanionStatus,
  hostExistingFirefoxSession: (serviceId) => hostBrowserCompanion(serviceId, null, "existingBrowser"),
  focus: focusDefaultBrowserCompanion,
  resize: resizeDefaultBrowserCompanion,
  detach: detachDefaultBrowserCompanion,
};

function initialState(): BrowserServiceQaState {
  return Object.freeze({
    phase: "idle",
    serviceId: null,
    browserId: null,
    sessionOwnership: "browser",
    credentialAccess: "none",
    captureProtected: false,
    containment: "bestEffort",
    error: null,
    revision: 0,
  });
}

function isTrustedDefaultFirefox(status: BrowserCompanionStatus): boolean {
  return status.status === "available"
    && status.browserId === "firefox"
    && status.reason === "none"
    && status.captureProtected === false
    && status.containment === "bestEffort";
}

function isExpectedAction(
  action: BrowserCompanionAction,
  expectedStatus: "hosted" | "focused" | "resized" | "detached",
): boolean {
  return action.status === expectedStatus
    && action.browserId === "firefox"
    && action.reason === "none"
    && action.mode === "existingBrowserCompanion"
    && action.captureProtected === false
    && action.containment === "bestEffort";
}

/**
 * Creates a narrow orchestration layer for deterministic browser-service QA.
 * Its public methods accept no URLs, selectors, scripts, credentials, cookies,
 * tokens, profile paths, or executable paths.
 */
export function createBrowserServiceQaShell(
  port: BrowserServiceQaPort = productionPort,
): BrowserServiceQaShell {
  let state = initialState();
  let operationPending = false;
  const listeners = new Set<(state: BrowserServiceQaState) => void>();

  const publish = (patch: Partial<BrowserServiceQaState>): BrowserServiceQaState => {
    state = Object.freeze({ ...state, ...patch, revision: state.revision + 1 });
    listeners.forEach((listener) => listener(state));
    return state;
  };

  const rejectWhilePending = (): BrowserServiceQaState => Object.freeze({
    ...state,
    error: "operationInProgress",
  });

  const runHostedAction = async (
    operation: () => Promise<BrowserCompanionAction>,
    expectedStatus: "focused" | "resized",
    error: "focusRejected" | "resizeRejected",
  ): Promise<BrowserServiceQaState> => {
    if (operationPending) return rejectWhilePending();
    if (state.phase !== "hosted" || state.serviceId === null) return publish({ phase: "failed", error });
    operationPending = true;
    try {
      const action = await operation();
      return isExpectedAction(action, expectedStatus)
        ? publish({ phase: "hosted", browserId: "firefox", error: null })
        : publish({ phase: "failed", error });
    } catch {
      return publish({ phase: "failed", error });
    } finally {
      operationPending = false;
    }
  };

  return {
    snapshot: () => state,

    async open(serviceId): Promise<BrowserServiceQaState> {
      if (operationPending) return rejectWhilePending();
      if (!supportedServices.has(serviceId)) {
        return publish({ phase: "failed", serviceId: null, browserId: null, error: "unsupportedService" });
      }
      operationPending = true;
      publish({ phase: "checking", serviceId, browserId: null, error: null });
      try {
        const status = await port.inspectDefaultBrowser();
        if (!isTrustedDefaultFirefox(status)) {
          return publish({ phase: "failed", browserId: null, error: "defaultFirefoxUnavailable" });
        }
        publish({ phase: "opening", browserId: "firefox", error: null });
        const action = await port.hostExistingFirefoxSession(serviceId);
        return isExpectedAction(action, "hosted")
          ? publish({ phase: "hosted", browserId: "firefox", error: null })
          : publish({ phase: "failed", browserId: null, error: "hostRejected" });
      } catch {
        return publish({ phase: "failed", browserId: null, error: "hostRejected" });
      } finally {
        operationPending = false;
      }
    },

    focus: () => runHostedAction(() => port.focus(), "focused", "focusRejected"),
    resize: () => runHostedAction(() => port.resize(), "resized", "resizeRejected"),

    async detach(): Promise<BrowserServiceQaState> {
      if (operationPending) return rejectWhilePending();
      if (state.phase !== "hosted" || state.serviceId === null) {
        return publish({ phase: "failed", error: "detachRejected" });
      }
      operationPending = true;
      try {
        const action = await port.detach();
        return isExpectedAction(action, "detached")
          ? publish({ phase: "idle", serviceId: null, browserId: null, error: null })
          : publish({ phase: "failed", error: "detachRejected" });
      } catch {
        return publish({ phase: "failed", error: "detachRejected" });
      } finally {
        operationPending = false;
      }
    },

    subscribe(listener): () => void {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
}
