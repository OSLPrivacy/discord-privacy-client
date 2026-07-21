import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("./preferences", () => ({ isTauriRuntime: mocks.isTauriRuntime }));

import {
  appendScrubIndexChunk,
  cancelScrubIndex,
  getScrubIndexStatus,
  getScrubIndexScan,
  initializeScrubIndex,
  parseScrubIndexStatus,
  pauseScrubIndex,
  resumeScrubIndex,
} from "./scrub-index";

const importId = "0123456789abcdef0123456789abcdef";
const status = {
  importId,
  phase: "running",
  source: "explicit_export",
  selectedAccountCount: 1,
  messagesIndexed: 0,
  findingsIndexed: 0,
  rejectedMessages: 0,
  completedChunks: 0,
  nextSequence: 0,
  bytesStored: 0,
  maxBytes: 50 * 1024 * 1024,
  analysisLocation: "this_device_only",
  persistedEncrypted: true,
  deletionEnabled: false,
} as const;

describe("Scrub index IPC", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.isTauriRuntime.mockReturnValue(true);
  });

  it("initializes only strict selected local profiles", async () => {
    mocks.invoke.mockResolvedValue(status);
    const request = {
      selections: [{ serviceId: "discord", accountId: "account-1" }],
      source: "explicit_export" as const,
    };
    await expect(initializeScrubIndex(request)).resolves.toEqual(status);
    expect(mocks.invoke).toHaveBeenCalledWith("initialize_scrub_index", { request });
    await expect(initializeScrubIndex({ ...request, selections: [...request.selections, ...request.selections] }))
      .rejects.toThrow("invalid");
  });

  it("streams one bounded chunk and never grants deletion authority", async () => {
    mocks.invoke.mockResolvedValue({ ...status, messagesIndexed: 1, findingsIndexed: 1, completedChunks: 1, nextSequence: 1 });
    const request = {
      importId,
      sequence: 0,
      finalChunk: false,
      messages: [{ serviceId: "discord", accountId: "account-1", conversationId: "chat-1", messageLocator: "msg-1", authoredBySelf: true, createdAtUnixMs: null, text: "local" }],
    };
    const result = await appendScrubIndexChunk(request);
    expect(result.deletionEnabled).toBe(false);
    expect(mocks.invoke).toHaveBeenCalledWith("append_scrub_index_chunk", { request });
  });

  it("strictly parses the authenticated status contract", () => {
    expect(parseScrubIndexStatus(status)).toEqual(status);
    expect(() => parseScrubIndexStatus({ ...status, deletionEnabled: true })).toThrow();
    expect(() => parseScrubIndexStatus({ ...status, unexpected: "field" })).toThrow();
    expect(() => parseScrubIndexStatus({ ...status, bytesStored: 50 * 1024 * 1024 + 1 })).toThrow();
  });

  it("supports status, pause, resume, and exact cancellation", async () => {
    mocks.invoke
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({ ...status, phase: "paused" })
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(undefined);
    await expect(getScrubIndexStatus()).resolves.toBeNull();
    await expect(pauseScrubIndex(importId)).resolves.toMatchObject({ phase: "paused" });
    await expect(resumeScrubIndex(importId)).resolves.toMatchObject({ phase: "running" });
    await expect(cancelScrubIndex(importId)).resolves.toBeUndefined();
    expect(mocks.invoke).toHaveBeenLastCalledWith("cancel_scrub_index", { importId });
  });

  it("reads only a strict persisted scan from the encrypted index", async () => {
    const scan = {
      findings: [], messagesScanned: 1, messagesRejected: 0, truncated: false,
      analysisLocation: "this_device_only", persisted: true,
    } as const;
    mocks.invoke.mockResolvedValue(scan);
    await expect(getScrubIndexScan(importId)).resolves.toEqual(scan);
    expect(mocks.invoke).toHaveBeenCalledWith("get_scrub_index_scan", { importId });
    mocks.invoke.mockResolvedValue({ ...scan, persisted: false });
    await expect(getScrubIndexScan(importId)).rejects.toThrow("invalid persisted");
  });

  it("fails closed outside the signed desktop runtime", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    await expect(getScrubIndexStatus()).rejects.toThrow("desktop app");
    expect(mocks.invoke).not.toHaveBeenCalled();
  });
});
