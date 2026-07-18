import type { Env } from "../env.js";
import { serviceUnavailable, tooMany } from "../lib/http.js";
import { callerIp, checkRateLimit } from "../lib/rate-limit.js";

export async function handleWindowsDownload(request: Request, env: Env): Promise<Response> {
  const limit = await checkRateLimit(env, callerIp(request), 10, "download-windows");
  if (!limit.ok) return tooMany(limit.retryAfter);
  if (!env.WINDOWS_INSTALLER_URL) {
    return serviceUnavailable("Windows installer is not configured");
  }
  let target: URL;
  try {
    target = new URL(env.WINDOWS_INSTALLER_URL);
  } catch {
    return serviceUnavailable("Windows installer is not configured");
  }
  if (target.protocol !== "https:" || target.hostname !== "installers.oslprivacy.com") {
    return serviceUnavailable("Windows installer is not configured");
  }
  await env.DB.prepare(
    `INSERT INTO download_events (event_id, artifact, created_at)
     VALUES (?, 'windows-msi', ?)`,
  ).bind(crypto.randomUUID(), Math.floor(Date.now() / 1000)).run();
  return new Response(null, {
    status: 302,
    headers: {
      location: target.toString(),
      "cache-control": "no-store",
      "x-content-type-options": "nosniff",
      "referrer-policy": "no-referrer",
    },
  });
}
