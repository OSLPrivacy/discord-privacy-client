# Onion routing decision

Cloudflare opportunistic onion routing may be enabled for website visitors, but
it is not OSL's client Tor solution and must not be advertised as one. It has
no cost, may help Tor Browser reach the website, and does not replace the T1
WAF-skip route to `keyserver.oslprivacy.com`.

The observable result is unknown until T11-D5 records `alt-svc` headers over
multiple Tor circuits. A normal `h3=":443"` header does not prove that an
onion alternative was emitted. Until that measurement exists, make no onion
availability claim.
