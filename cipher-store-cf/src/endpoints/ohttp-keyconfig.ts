/// OHTTP gateway key configuration is deliberately unavailable in v1.
///
/// D7 requires two genuinely independent operators for OHTTP.  Serving a
/// configuration from this Worker before an independently operated gateway
/// exists would advertise privacy the deployment cannot provide.  Keep this
/// helper separate from the router: it is a fail-closed placeholder, not a
/// public endpoint.
export function unavailableOhttpKeyConfig(): Response {
  return new Response(null, {
    status: 404,
    headers: { "cache-control": "no-store" },
  });
}
