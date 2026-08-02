# Seeded non-IMAP provider fixtures

These loopback-only HTTP fixtures are the sole Scrub targets for non-IMAP
provider adapters. They never contact a provider or accept real credentials.
The providers modelled are Reddit, X, Instagram, Gmail REST, Discord's
consented UI route, and Facebook/Instagram Activity.

X, Instagram, and Gmail REST are fixture-only regression routes: D56/D57
forbid their live deletion-API lanes. Their scripted responses let future
adapters prove they refuse or fail closed without ever reaching those APIs.

Every `enumerate`, `inspect`, `delete`, and `verify` endpoint supports the
same scripted `success`, `refusal`, `rate-limit`, and `ambiguous` result:

```text
GET    /v1/reddit/enumerate?outcome=success
GET    /v1/x/inspect?outcome=refusal
DELETE /v1/instagram/delete?outcome=rate-limit
GET    /v1/gmail-rest/verify?outcome=ambiguous
```

`ambiguous` returns HTTP 503 and explicitly says its state is unknown. It is
not success-shaped: an adapter must record `UNKNOWN` and stop without retry.
The real Reddit account mentioned in the earlier track text is deliberately
not used here; no throwaway credentials or real-account deletion target is
safe or required for this fixture contract.

Run the SCR-E5 contract:

```sh
pytest -q scripts/qa/scrub-provider-fixtures/test_fixture.py
```

For manual adapter work:

```sh
python3 scripts/qa/scrub-provider-fixtures/fixture_server.py --port 8787
```
