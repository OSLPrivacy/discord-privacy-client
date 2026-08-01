# OHTTP readiness: deliberately not deployed in v1

OSL does **not** provide OHTTP in v1. This is a product and deployment
constraint, not an unfinished relay: an OHTTP relay and gateway operated from
the same Cloudflare account, organisation, or operator do not create the
privacy split OHTTP is meant to provide.

Accordingly, `src/endpoints/ohttp-keyconfig.ts` is an unwired, fail-closed
stub. It returns no key-configuration bytes and must not be added to the
Worker router while this document says OHTTP is unavailable. A client must not
be told that OHTTP protects a request until all of the following are true.

## Required operator split

The relay and gateway must be run by genuinely separate operators, with
separate accounts, credentials, production access, logging authority, and
network administration. The relay may learn the client's network address and
the selected gateway; it must not decrypt the encapsulated request. The
gateway may decrypt and send the inner request but must see the relay's
address, not the client's address. Neither OSL nor its Cloudflare account may
operate both sides.

This is a separation of knowledge, not a guarantee against collusion. Users
must be able to identify the relay and gateway operators and make their own
trust decision about that risk.

## Conditions before enabling a key-config endpoint

1. Contract with an external relay and a separately operated gateway, with the
   split above verified in deployment review.
2. Deploy the HPKE private key only at the gateway. Do not put it in this
   Worker, its secrets, its repository, or the relay.
3. Publish the gateway's OHTTP key configuration from the gateway, with an
   authenticated rotation and retirement process. Clients must parse and
   reject malformed configurations before use.
4. Keep the cipher-store fetch exchange a single request/response: no
   streaming, cookies, or redirects. This is already the shape required for a
   future OHTTP encapsulation, but it does not make the current route OHTTP.
5. Add interoperability tests using public test vectors and a negative test
   for malformed HPKE key configurations. Do not create a production key or a
   relay merely to satisfy those tests.

Until these conditions are met, the correct behavior is absence: no public
OHTTP route, no gateway key configuration, and no OHTTP privacy claim.
