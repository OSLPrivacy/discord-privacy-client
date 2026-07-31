/**
 * Exact production seam observed by the D2 promotion proof.
 *
 * Wrangler cannot import JavaScript, so the Node closure gate separately
 * requires wrangler.toml's sole production trigger to equal this value.
 */
export const NATURAL_CRON = "*/5 * * * *";

/**
 * Fixed, identifier-free marker emitted only after the attachment sweep
 * completes successfully.
 */
export const CYCLE_MARKER = "[attachment-sweep-cycle] complete";
