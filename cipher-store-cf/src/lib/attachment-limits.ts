export const MAX_DIRECT_ATTACHMENT_BYTES = 26 * 1024 * 1024;
export const MAX_PLAINTEXT_ATTACHMENT_BYTES = 512 * 1024 * 1024;
// Leaves a bounded allowance for chunk framing and AEAD tags without asking
// the store to infer plaintext size from opaque ciphertext.
export const MAX_SEALED_ATTACHMENT_BYTES = 513 * 1024 * 1024;
export const MAX_ATTACHMENT_PART_BYTES = 8 * 1024 * 1024;
export const MAX_ATTACHMENT_PARTS = Math.ceil(
  MAX_SEALED_ATTACHMENT_BYTES / MAX_ATTACHMENT_PART_BYTES,
);

// These values are duplicated as CHECK constraints in migration 0004. The
// D1 trigger is authoritative and serializes concurrent reservations.
export const MAX_LIVE_ATTACHMENT_ROWS = 512;
export const MAX_LIVE_ATTACHMENT_BYTES = 8 * 1024 * 1024 * 1024;

export const ATTACHMENT_SWEEP_BATCH_SIZE = 100;
