/// Reservation gate for view-once attachment reads.
///
/// This operates on the attachment object, not on an HTTP request or range.
/// Every retry and every range in a transfer therefore shares one fixed claim.

import type { Env } from "../env.js";

/** The hard, non-extendable attachment retry window (features.md §4.3 C5). */
export const ATTACHMENT_RESERVATION_SECONDS = 60 * 60;

export interface AttachmentReservationRow {
  single_fetch: number;
  reserved_until: number | null;
}

export async function reserveAttachmentFetch(
  env: Env,
  id: string,
  row: AttachmentReservationRow,
  now: number,
): Promise<boolean> {
  if (row.single_fetch !== 1) return true;
  if (row.reserved_until !== null) return row.reserved_until >= now;

  // One CAS for the entire object transfer.  In particular, do not move this
  // inside range handling: doing so would make a part-2 retry consume a second
  // view-once claim.
  const claimed = await env.DB.prepare(
    `UPDATE attachment_objects
        SET reserved_until = ?
      WHERE id = ?
        AND state = 'ready'
        AND single_fetch = 1
        AND reserved_until IS NULL`,
  ).bind(now + ATTACHMENT_RESERVATION_SECONDS, id).run();
  if ((claimed.meta.changes ?? 0) === 1) return true;

  const current = await env.DB.prepare(
    `SELECT reserved_until FROM attachment_objects
      WHERE id = ? AND state = 'ready' AND single_fetch = 1 LIMIT 1`,
  ).bind(id).first<{ reserved_until: number | null }>();
  // REAL DEFECT, not type noise. This line was:
  //
  //   return current?.reserved_until !== null && current.reserved_until >= now;
  //
  // `.first()` returns null when no row matches. The optional chain on the
  // left then yields `undefined`, `undefined !== null` is TRUE, and the
  // right-hand side dereferences `current` -- which is null -- and throws a
  // TypeError. The optional chain guarded only the read it was written on and
  // then handed a null through to an unguarded one.
  //
  // The path is live: we reach here only when the CAS above changed no row, and
  // "no row" is exactly what the attachment sweep leaves behind. A view-once
  // attachment swept between the UPDATE and this SELECT crashed
  // handleAttachmentFetch (no try/catch at the call site) instead of returning
  // the 404 the caller already handles. Absent row now means "no live
  // reservation" -- false -- which is what the boolean was always for.
  return current !== null && current.reserved_until !== null && current.reserved_until >= now;
}
