# Seeded IMAP fixture

This directory is the only IMAP target for Scrub QA. `seed.py` and `verify.py`
refuse any host other than loopback or the two Compose service names, so they
cannot be aimed at a real mailbox.

Start the two deliberately different local targets:

```sh
docker compose -f scripts/qa/scrub-imap-fixture/compose.yml up -d
python3 scripts/qa/scrub-imap-fixture/seed.py --port 1143 --messages 3
python3 scripts/qa/scrub-imap-fixture/verify.py --port 1143 --messages 3
python3 scripts/qa/scrub-imap-fixture/seed.py --port 2143 --messages 3
python3 scripts/qa/scrub-imap-fixture/verify.py --port 2143 --messages 3
```

Both targets use Dovecot's image-default `fixture`/`pass` test identity.
The `uidplus` target is unmodified; `nouidplus` deliberately omits UIDPLUS
from its advertised IMAP capabilities. Seeding also creates `[Gmail]/All Mail`,
`[Gmail]/Starred`, and `[Gmail]/Trash` to supply a Gmail-like folder hierarchy.

Run the contractual test (including the wrong-port sabotage check) with:

```sh
pytest -q scripts/qa/scrub-imap-fixture/test_fixture.py
```
