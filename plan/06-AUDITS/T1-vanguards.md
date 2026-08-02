# T1-73 — Vanguards finding: no OSL toggle

**Status:** closed 2026-08-02.  This records R4; it is not a new
investigation or a product feature.

## Finding

OSL must not expose a Vanguards setting or claim that it applies to its normal
store traffic.  The Tor Vanguards specification states that neither Vanguards
system applies to exit activity.  OSL's current HTTPS cipher-store route is
exit activity, so a preference would be security theatre and a false claim.

Vanguards-Lite is a Tor default (since 0.4.7), and Arti enables it by default
for onion-service circuits.  If T1-74 enables Cloudflare Onion Routing, that
onion path receives the relevant protection without an OSL-specific toggle.
This does not change the result for exit traffic and does not authorize an
application preference.

## Product boundary

- No UI, configuration, capability, or onboarding toggle is permitted.
- T1-71/T1-72 retain the only user-facing routing choice: direct versus Tor.
- T1-74 may configure the service's onion route, but it must not represent
  that as a Vanguards control.
- Public claims must retain the limits in `T1-tor-spike.md`: Tor hides the
  client IP from OSL but not the existence of a persistent connection.

## Evidence

This is the R4 conclusion already recorded in
[`T1-tor-spike.md`](T1-tor-spike.md): the Tor Vanguards specification confines
both systems away from exit activity.  Primary reference:
<https://spec.torproject.org/vanguards-spec/>.
