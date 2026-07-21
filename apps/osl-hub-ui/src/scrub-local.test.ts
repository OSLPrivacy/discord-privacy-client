import { describe, expect, it, vi } from "vitest";
import type { LocalMessageCandidate, PersistedLocalPrivacyScanResult } from "./adapters";
import { localImportCoverageReceipt, persistLocalScrubExport, type LocalScrubIndexAdapters } from "./scrub-local";
import type { ScrubIndexStatus } from "./scrub-index";

const importId = "0123456789abcdef0123456789abcdef";
const message = (index: number): LocalMessageCandidate => ({
  serviceId: "local_import",
  accountId: "manual-export",
  conversationId: "privacy-scan",
  messageLocator: `message-${index}`,
  authoredBySelf: false,
  createdAtUnixMs: index === 0 ? null : 1_700_000_000_000 + index,
  text: `local text ${index}`,
});
const status = (overrides: Partial<ScrubIndexStatus> = {}): ScrubIndexStatus => ({
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
  ...overrides,
});
const scan = (messagesScanned: number): PersistedLocalPrivacyScanResult => ({
  findings: [], messagesScanned, messagesRejected: 0, truncated: false,
  analysisLocation: "this_device_only", persisted: true,
});

describe("local Scrub persisted pipeline", () => {
  it("initializes, chunks, verifies status, then reads findings only from the persisted index", async () => {
    const messages = Array.from({ length: 257 }, (_, index) => message(index));
    const complete = status({ phase: "complete", messagesIndexed: 257, completedChunks: 2, nextSequence: 2 });
    const adapters: LocalScrubIndexAdapters = {
      getStatus: vi.fn().mockResolvedValueOnce(null).mockResolvedValueOnce(complete),
      initialize: vi.fn().mockResolvedValue(status()),
      resume: vi.fn(),
      cancel: vi.fn(),
      append: vi.fn().mockResolvedValueOnce(status({ messagesIndexed: 256, completedChunks: 1, nextSequence: 1 })).mockResolvedValueOnce(complete),
      readScan: vi.fn().mockResolvedValue(scan(257)),
    };

    const result = await persistLocalScrubExport(messages, adapters);
    expect(adapters.initialize).toHaveBeenCalledWith({
      selections: [{ serviceId: "local_import", accountId: "manual-export" }],
      source: "explicit_export",
    });
    expect(adapters.append).toHaveBeenNthCalledWith(1, expect.objectContaining({ sequence: 0, finalChunk: false, messages: messages.slice(0, 256) }));
    expect(adapters.append).toHaveBeenNthCalledWith(2, expect.objectContaining({ sequence: 1, finalChunk: true, messages: messages.slice(256) }));
    expect(adapters.readScan).toHaveBeenCalledWith(importId);
    expect(result.scan.persisted).toBe(true);
    expect(result.status.deletionEnabled).toBe(false);
    expect(result.receipt).toMatchObject({ providerReportedComplete: false, textChecked: true, imagesChecked: false });
  });

  it("replays chunks from sequence zero so an interrupted identical import resumes idempotently", async () => {
    const messages = Array.from({ length: 257 }, (_, index) => message(index));
    const partial = status({ messagesIndexed: 256, completedChunks: 1, nextSequence: 1 });
    const complete = status({ phase: "complete", messagesIndexed: 257, completedChunks: 2, nextSequence: 2 });
    const adapters: LocalScrubIndexAdapters = {
      getStatus: vi.fn().mockResolvedValueOnce(partial).mockResolvedValueOnce(complete),
      initialize: vi.fn(),
      resume: vi.fn(),
      cancel: vi.fn(),
      append: vi.fn().mockResolvedValueOnce(partial).mockResolvedValueOnce(complete),
      readScan: vi.fn().mockResolvedValue(scan(257)),
    };
    await persistLocalScrubExport(messages, adapters);
    expect(adapters.initialize).not.toHaveBeenCalled();
    expect(adapters.append).toHaveBeenNthCalledWith(1, expect.objectContaining({ sequence: 0 }));
    expect(adapters.append).toHaveBeenNthCalledWith(2, expect.objectContaining({ sequence: 1 }));
  });

  it("emits a validated incomplete receipt for manual exports and records timestamp gaps", () => {
    const receipt = localImportCoverageReceipt([message(0), message(2)], 2);
    expect(receipt.oldestReachableAtUnixMs).toBe(1_700_000_000_002);
    expect(receipt.newestReachableAtUnixMs).toBe(1_700_000_000_002);
    expect(receipt.providerReportedComplete).toBe(false);
    expect(receipt.gaps).toHaveLength(2);
  });

  it("resumes a paused import before replaying its chunks", async () => {
    const paused = status({ phase: "paused" });
    const complete = status({ phase: "complete", messagesIndexed: 1, completedChunks: 1, nextSequence: 1 });
    const adapters: LocalScrubIndexAdapters = {
      getStatus: vi.fn().mockResolvedValueOnce(paused).mockResolvedValueOnce(complete),
      initialize: vi.fn(),
      resume: vi.fn().mockResolvedValue(status()),
      cancel: vi.fn(),
      append: vi.fn().mockResolvedValue(complete),
      readScan: vi.fn().mockResolvedValue(scan(1)),
    };
    await persistLocalScrubExport([message(0)], adapters);
    expect(adapters.resume).toHaveBeenCalledWith(importId);
    expect(adapters.append).toHaveBeenCalledWith(expect.objectContaining({ sequence: 0, finalChunk: true }));
  });

  it("cancels a completed index before initializing a different export", async () => {
    const previous = status({ phase: "complete", messagesIndexed: 1, completedChunks: 1, nextSequence: 1 });
    const complete = status({ phase: "complete", messagesIndexed: 1, completedChunks: 1, nextSequence: 1 });
    const adapters: LocalScrubIndexAdapters = {
      getStatus: vi.fn().mockResolvedValueOnce(previous).mockResolvedValueOnce(complete),
      initialize: vi.fn().mockResolvedValueOnce(previous).mockResolvedValueOnce(status()),
      resume: vi.fn(),
      cancel: vi.fn().mockResolvedValue(undefined),
      append: vi.fn().mockResolvedValue(complete),
      readScan: vi.fn().mockResolvedValue(scan(1)),
    };
    await persistLocalScrubExport([message(0)], adapters);
    expect(adapters.cancel).toHaveBeenCalledWith(importId);
    expect(adapters.initialize).toHaveBeenCalledTimes(2);
    const initializeOrder = (adapters.initialize as ReturnType<typeof vi.fn>).mock.invocationCallOrder;
    const cancelOrder = (adapters.cancel as ReturnType<typeof vi.fn>).mock.invocationCallOrder;
    expect(initializeOrder[0]).toBeLessThan(cancelOrder[0]);
    expect(cancelOrder[0]).toBeLessThan(initializeOrder[1]);
  });

  it("does not cancel an unrelated persisted index", async () => {
    const unrelated = status({ phase: "complete", source: "osl_visible_data" });
    const adapters: LocalScrubIndexAdapters = {
      getStatus: vi.fn().mockResolvedValue(unrelated),
      initialize: vi.fn(), resume: vi.fn(), cancel: vi.fn(), append: vi.fn(), readScan: vi.fn(),
    };
    await expect(persistLocalScrubExport([message(0)], adapters)).rejects.toThrow("different Scrub import");
    expect(adapters.cancel).not.toHaveBeenCalled();
    expect(adapters.initialize).not.toHaveBeenCalled();
  });
});
