import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn(), isTauriRuntime: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import { listHubIdentities } from "./adapters";
import { clearBackendFailures, setBackendFailureConsole } from "./backend-failure";

// The shape `list_hub_identities` actually returns for a real account: one
// slot for the identity that was created during onboarding.
const slot = { slotId: "id-YO3kIbux4ViXwYnAOoLm_x-_", label: "Primary identity", oslUserId: "osl_9c1f2b", active: true };

describe("list_hub_identities response", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
    clearBackendFailures();
    setBackendFailureConsole(false);
  });

  it("accepts the slot the backend sends for a real account", async () => {
    mocks.invoke.mockResolvedValueOnce([slot]);
    await expect(listHubIdentities()).resolves.toEqual([slot]);
  });

  it("rejects a slot carrying a safety number it cannot have derived", async () => {
    // The registry stores only the Ed25519 half of the key bundle, so it can
    // never produce a comparable safety number. A slot that claims the field
    // is a contract violation, not something to render.
    mocks.invoke.mockResolvedValueOnce([{ ...slot, safetyNumber: "" }]);
    await expect(listHubIdentities()).resolves.toBeNull();
    mocks.invoke.mockResolvedValueOnce([{ ...slot, safetyNumber: "01234 56789" }]);
    await expect(listHubIdentities()).resolves.toBeNull();
  });

  it("reports a refused command as null rather than an empty account", async () => {
    mocks.invoke.mockRejectedValueOnce(new Error("OSL main password must be unlocked"));
    await expect(listHubIdentities()).resolves.toBeNull();
  });
});
