// Hand-authored declarations for scripts/deletion-durability-probe.mjs.
//
// test-node/deletion-durability-probe.test.ts imports `runProbe` from the .mjs
// directly and, with no declaration present, TS7016 gave the whole probe an
// implicit `any` -- so the receipt fields the test asserts on (`r2HeadStatus`,
// `lockRules`) were unchecked, and so was every argument passed in. These
// signatures are transcribed from the .mjs, not widened to fit the call sites.

/** Parsed and validated CLI arguments. `worker` is normalised by previewWorkerUrl. */
export interface DeletionProbeArgs {
  worker: string;
  accountId: string;
  bucket: string;
  yes: boolean;
}

export interface DeletionProbeOptions {
  /** HTTPS workers.dev preview URL. Custom domains are refused. */
  worker: string;
  accountId: string;
  bucket: string;
  /** CLOUDFLARE_API_TOKEN. Absent throws before any request is made. */
  token: string | undefined;
  /** Defaults to global fetch. */
  fetchImpl?: typeof fetch;
  /** Defaults to https://api.cloudflare.com/client/v4 */
  apiBase?: string;
}

/** The receipt returned on success; the probe throws on every failure path. */
export interface DeletionProbeReceipt {
  blobId: string;
  objectKey: string;
  r2HeadStatus: number;
  lockRules: number;
  versioning: "unsupported-by-r2";
}

/** Throws on an unknown flag or a missing/non-preview --worker. */
export function parseArgs(argv: readonly string[]): DeletionProbeArgs;

export function runProbe(options: DeletionProbeOptions): Promise<DeletionProbeReceipt>;
