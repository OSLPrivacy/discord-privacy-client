import type { Env } from "../env.js";
import { cleanupExpiredAndAbandonedAttachments } from "./attachment-cleanup.js";

/**
 * The scheduled-handler leg of attachment cleanup. Keeping this small seam
 * separate makes the cron's observable completion record independently
 * testable even when unrelated HTTP routes are unavailable to compile.
 */
export async function runAttachmentCleanupTimer(env: Env, scheduledTime: number): Promise<void> {
  try {
    const deleted = await cleanupExpiredAndAbandonedAttachments(env);
    // The count is deliberately exact and contains no attachment identity.
    // It makes every scheduled cleanup run auditable without retaining
    // identifiers, object names, or user data in the log line.
    console.log(`[attachment-cleanup] scheduled_at=${scheduledTime} deleted=${deleted}`);
  } catch {
    console.error("[attachment-cleanup] failed");
  }
}
