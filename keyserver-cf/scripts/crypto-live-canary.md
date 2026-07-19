# Private BTC/XMR release canary client

`crypto-live-canary.mjs` is an operator-only Node 24 client for the two paid
release canaries. It talks only to `https://keyserver.oslprivacy.com`. It has no
wallet RPC, seed, signing, transaction-building, or spending capability.

Do not use it until the watcher is live and the Worker has enabled only the
asset being tested. Keep the public website checkout disabled during the
canary. Send the exact displayed amount from an independent wallet.

Create one invoice:

```sh
umask 077
node scripts/crypto-live-canary.mjs create \
  --asset btc \
  --state /path/on/encrypted-disk/osl-btc-canary.json
```

Use `--asset xmr` for the Monero canary. The state file contains the anonymous
claim token and the locally generated RSA-OAEP private delivery key. The client
creates it with mode `0600` and never prints either value. Do not copy it into
chat, logs, shell arguments, source control, or cloud storage.

After manually sending the exact amount, resume safely at any time:

```sh
node scripts/crypto-live-canary.mjs watch \
  --state /path/on/encrypted-disk/osl-btc-canary.json \
  --activation-out /path/on/encrypted-disk/osl-btc-activation.txt
```

The watcher command polls the exact invoice, decrypts the activation locally,
and requires an exact version-1 JSON envelope binding the activation to the
retained invoice ID, payment asset, $5 amount, and Pro plan. Extra, missing, or
swapped valid fields fail before public validation, output, or acknowledgement.
The client then requires the public validation endpoint to return active
lifetime Pro and atomically creates the separate activation output with mode
`0600`. It never prints the activation. Only after the output is durably
flushed does it acknowledge delivery. Acknowledgement destroys the server-side encrypted
delivery. The claim/key state is removed only after that complete sequence. An
expired response is reconciliation-required, not a deletion signal: the client
keeps the state so an observed late payment can still be reconciled without
losing its claim or delivery key. Other failures likewise leave the state
available for a safe retry. The output path must not already contain unrelated
data; an existing byte-identical activation is accepted only to resume an
acknowledgement whose response was interrupted. Before acknowledgement, the
client durably binds the exact output path and activation-content hash into the
mode-`0600` state. If the server commits acknowledgement but its HTTP response
is lost, a retry re-reads that exact output, verifies the binding, revalidates
the activation as lifetime Pro, accepts only the server's exact acknowledged
`410` response, and then removes the claim/key state.

The 24-hour grace limit stops continuous polling only. Every manual `watch`
invocation makes at least one authoritative status request, even after that
deadline, so a preserved invoice can recover a delivery produced by later
operator reconciliation.

New secret files are not considered created until both the file and parent
directory have been synchronized. State changes use a mode-`0600`, exclusive,
same-directory temporary file, synchronize it, atomically rename it over the
old state, and then synchronize the parent directory. A failed pre-rename
update leaves the complete old state and removes the temporary file best
effort; a post-rename failure still exposes only the complete replacement and
stops before any acknowledgement.

Focused offline verification (HTTP, clock, delays, and business-path storage
are substituted; filesystem durability checks use only temporary local files):

```sh
node --test scripts/crypto-live-canary.test.mjs
```

The tests prove strict response schemas, fixed endpoints, bounded rate-limit
handling, no secret output, and that acknowledgement cannot occur before both
local RSA decryption and public activation validation succeed. They also cover
parent-directory synchronization, atomic-replacement crash boundaries, and
late-payment state preservation.
