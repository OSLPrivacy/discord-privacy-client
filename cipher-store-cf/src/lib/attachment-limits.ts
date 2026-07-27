export const MAX_DIRECT_ATTACHMENT_BYTES = 26 * 1024 * 1024;
// Retained for older importers; current Worker enforcement uses sealed-size
// limits and never reads plaintext attachment size.
export const MAX_PLAINTEXT_ATTACHMENT_BYTES = 512 * 1024 * 1024;
// Leaves a bounded allowance for chunk framing and AEAD tags without asking
// the store to infer plaintext size from opaque ciphertext.
export const MAX_SEALED_ATTACHMENT_BYTES = 513 * 1024 * 1024;
export const MAX_ATTACHMENT_PART_BYTES = 8 * 1024 * 1024;
export const MAX_ATTACHMENT_PARTS = Math.ceil(
  MAX_SEALED_ATTACHMENT_BYTES / MAX_ATTACHMENT_PART_BYTES,
);

// Aggregate quota enforcement lives only in the Worker's `insertObject`
// conditional `INSERT INTO attachment_objects ... SELECT ... WHERE` statement.
// D1 serializes that single write statement; migration 0004 creates no trigger,
// and there is no trigger or CHECK constraint for the row/byte aggregates.
export const MAX_LIVE_ATTACHMENT_ROWS = 512;
export const MAX_LIVE_ATTACHMENT_BYTES = 8 * 1024 * 1024 * 1024;

// --- Incomplete multipart sessions (audit HIGH-1) -------------------------
//
// A session reserves its DECLARED size before any ciphertext exists, so
// reservations must live in their own small pool rather than competing with
// stored content. The numbers below are chosen so that an attacker who fills
// the reservation pool completely still leaves 448 rows and 6 GiB available to
// ordinary uploads, and so that the damage self-heals within one sweep cycle
// of the hold expiring.

/// How long an unfinished session may hold its reservation. Each accepted part
/// slides this forward, so a slow but genuine upload is never cut off; only a
/// session that stops making progress is reclaimed.
export const INCOMPLETE_SESSION_TTL_SECONDS = 15 * 60;

/// Reservation-pool ceilings, applied only to rows whose state is not `ready`.
/// Four concurrent full-size (512 MiB) uploads fit; the global backstop above
/// still applies on top.
export const MAX_INCOMPLETE_SESSION_ROWS = 64;
export const MAX_INCOMPLETE_SESSION_BYTES = 2 * 1024 * 1024 * 1024;

export const ATTACHMENT_SWEEP_BATCH_SIZE = 100;
