/// The blob fetch surface is intentionally constrained to an OHTTP-compatible
/// request/response exchange. Keep this separate from the realtime transport:
/// that channel deliberately has a different, long-lived shape.

const FETCH_CAP_RE = /^[0-9a-f]{32}$/;

/**
 * A fetch capability is bearer authority, so it may only travel in its
 * dedicated header. In particular, query parameters would make a path-visible
 * blob id into a logged, live capability URL.
 */
export function isOhttpReadyBlobFetch(request: Request): boolean {
  const url = new URL(request.url);
  const fetchCap = request.headers.get("x-osl-fetch-cap")?.trim().toLowerCase();
  return request.method === "GET"
    && url.search === ""
    && request.body === null
    && Boolean(fetchCap && FETCH_CAP_RE.test(fetchCap));
}

/** A bounded, cookie-free, non-redirecting OHTTP fetch response. */
export function ohttpBlobFetchResponse(bytes: Uint8Array): Response {
  return new Response(bytes, {
    status: 200,
    headers: {
      "content-type": "application/octet-stream",
      "cache-control": "no-store",
      "content-length": String(bytes.byteLength),
    },
  });
}

