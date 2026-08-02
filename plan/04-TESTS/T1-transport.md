# T1 transport test suite — revision 2

**Status:** planned against the jointly re-frozen transport contract (2026-07-31, revision 2).

## Authority and test rules

This suite is governed by `plan/03-CONTRACTS/transport.md` and, where they conflict,
`09-DECISIONS.md` (D1–D11 and D28–D30).  The storage derivations and blob-route
semantics have one normative definition in `storage.md`; tests consume shared vectors rather
than duplicating that derivation in a second implementation.

All capability values in fixtures are deterministic, labelled test vectors.  Production tests
must never log `P`, any capability, plaintext, or a real account identifier.  Negative-route
comparisons assert the complete HTTP response (status, headers, and body), not merely its status.
Live/store tests are opt-in and must use an isolated namespace and expiry-safe cleanup.

### Revision-2 exclusion gate

No T1 test may encode the superseded r1 protocol.  In particular, there is no success case for:

- a fetch capability in a URL or a fetch without `X-OSL-Fetch-Cap`;
- a server-assigned blob id, a directly-used public carrier capability, or a random
  server-minted manage token;
- `GET /v1/blob/:id/status` or any sender-visible ack state;
- distinguishable `400`, `401`, or `403` blob-route failures; or
- ACK authorization with `fetch_cap`, acknowledgement before durable persistence, or a
  frame that authorizes a fetch.

Each implementation test below includes its named sabotage.  The owning task must perform that
mutation or an equivalent behavioural fault and record a red result before restoring green.

## Shared fixtures and gates

| fixture / gate | owner | required assertions |
|---|---|---|
| `T1-F01` seed/capability vectors | T6-R1, consumed by T1 | 20-byte `P` produces separate 16-byte `blob_id` and `fetch_cap`; `ack_cap` and `manage_cap` require their respective secret keys; outputs change with each input domain. |
| `T1-F02` delivery/detection vectors | T1-03 + T6-W1 | shared epoch-secret vectors prove `info="osl/tag/v1"` and `info="osl/detect/v1"` are domain-separated and the public pointer cannot produce `delivery_tag`. |
| `T1-F03` blob worker harness | T1-20 | captures complete negative HTTP responses and request observables; supports PUT, GET, ACK and unconditional BURN. |
| `T1-F04` pointer/cover vectors | T1-30/31/32 | deterministic carrier codec fixtures plus a CSPRNG seam for uniqueness tests; server-observable requests must exclude `P` and `detect_tag`. |
| `T1-F05` frame transcript harness | T1-50 | fixed-size text frames, tick timestamps, reconnect/replay transcript, and a fake store client. |
| `T1-F06` two-client fixture | T1-80 | isolated sender/recipient stores, durable-write fault injection, adapter disable switch, and captured server observables. |

`T1-31b` is a hard gate: no test or implementation may bless the 160-bit carrier until its
measurement is recorded as passed by the owner.  The test plan still reserves its test ID so the
decision is auditable.

## Test-ID matrix

| ID | task | proof | required red sabotage |
|---|---|---|---|
| T1-T03 | T1-03 | Shared `T1-F02` vectors derive stable expected tag and detect outputs from one epoch secret while proving they differ from one another. | Change one HKDF `info` string. |
| T1-T05 | T1-05 | Review verifies the Tor crate is claimed and stale dependency note is absent. | Reintroduce the stale claim. |
| T1-T10 | T1-10 | PUT accepts a well-formed client 16-byte `blob_id` and capability digests, never receives a server-assigned id, and rejects legacy 8-byte names. | Restore id minting or accept an 8-byte id. |
| T1-T12 | T1-12 | A matching derived `manage_cap` burns idempotently; `fetch_cap` and arbitrary headers cannot burn. | Authorize DELETE with `fetch_cap`. |
| T1-T13 | T1-13 | Valid Padmé lengths are accepted and an unpadded 1,001-byte body is refused. | Accept arbitrary lengths. |
| T1-T15 | T1-15 | Malformed id, absent/malformed/wrong cap, missing, acked, and burned GET/ACK objects have byte-identical 404 responses. | Restore a `400`, `401`, or `403` branch. |
| T1-T16 | T1-16 | A bounded series of misses is rate-limited without changing the negative 404 surface. | Raise the fetch ceiling by 1,000×. |
| T1-T17 | T1-17 | Decoy misses use the ordinary fetch path and are indistinguishable from wrong/missing-cap misses. | Add a decoy-specific worker branch. |
| T1-T18 | T1-18 | Fetch profile is one self-contained request/response with `blob_id` only in the path, `X-OSL-Fetch-Cap` only in the header, and no cookie, redirect, or streaming. | Add `Set-Cookie` or a redirect. |
| T1-T19 | T1-19 | Served HPKE key configuration parses and the two-operator deployment precondition is documented. | Serve malformed key configuration. |
| T1-T20 | T1-20 | Worker regression suite exercises revision-2 PUT/GET/ACK/BURN wire behaviour using `T1-F01` and `T1-F03`. | Run assertions against the pre-T1-10 implementation. |
| T1-T30 | T1-30 | Two sends produce independent fresh seeds and derived authorities; server capture contains neither `P` nor `detect_tag`. | Reintroduce scope-derived material or transmit `P`. |
| T1-T31 | T1-31 | A party knowing only public scope data cannot recognise a pointer; a holder of `K_detect` can. | Restore `prose_token_salt(&scope)`. |
| T1-T31b | T1-31b | Record rendered character, word, and line counts at 96/128/160 bits against the 96-bit baseline; owner decides pass/fail. | Not applicable: measurement, not a pass-by-construction unit test. |
| T1-T32 | T1-32 | Codec round-trip is exactly `[P:20][detect_tag:4]`; neither component reaches the server. | Leave the 8-byte carrier or encode a bearer cap. |
| T1-T33 | T1-33 | Re-budgeted cover codec round-trips the 24-byte pointer within carrier constraints. | Shrink the row budget by one. |
| T1-T34 | T1-34 | Every client upload length is a valid Padmé length. | Return the input unchanged. |
| T1-T35 | T1-35 | Restarted burn work recomputes the same `manage_cap` from recoverable sender state and `blob_id`. | Require a persisted/server-minted random token. |
| T1-T36 | T1-36 | Shipping send-path records all blob identities needed for a scope burn; burn deletes each recorded blob. | Skip the recording call. |
| T1-T37 | T1-37 | Member B cannot decrypt member A's manifest entry and can still fetch after member A fetches and ACKs. | Allow ACK to destroy the shared manifest or use one sealing key. |
| T1-T41 | T1-41 | Public send/receive integration has no reachable inline payload path. | Re-add an inline send caller. |
| T1-T42 | T1-42 | Pointer transport cannot invoke payload chunking or reassembly. | Re-add a chunking call. |
| T1-T43 | T1-43 | Cover length is invariant for different payload sizes because it carries only a fixed pointer. | Reintroduce a payload-length branch. |
| T1-T44 | T1-44 | Documentation review confirms only pointer transport and current constants are described. | Reintroduce a retired transport claim. |
| T1-T45 | T1-45 | Integration review records the excluded `src-tauri` boundary and no silent edit to it. | Expand scope into the excluded shell without a recorded decision. |
| T1-T50 | T1-50 | `T1-F05` vectors prove fixed-size text wakeups contain only `{delivery_tag, blob_id}`, never `P`, capabilities, identity, payload, or conversation id. | Put a capability in a frame. |
| T1-T51 | T1-51 | Server answers client ticks with equal-size frames and does not require a server alarm per tick. | Replace auto-response with a server-side tick. |
| T1-T52 | T1-52 | Across 50 hit and miss frames, client outbound size and cadence remain constant; an unmatched wakeup does not fetch. | Make ticking depend on whether the prior frame had a hit. |
| T1-T53 | T1-53 | 1,000 simulated reconnects are jittered, replayed, and resume without a 100 ms retry herd. | Remove jitter. |
| T1-T54 | T1-54 | UI makes no timed polling invoke and focus/open events do not produce a fetch. | Reinstate `idlePollMs`. |
| T1-T55 | T1-55 | Subscription requests contain only rotating tag windows; account identifiers never reach the channel. | Subscribe by `osl_user_id`. |
| T1-T56 | T1-56 | Decoy frames and follow-on fake fetches have ordinary fixed cadence/size and ordinary 404 misses. | Give decoys a distinct cadence or size. |
| T1-T62 | T1-62 | Offline send queue and receiver journal never evict a live record when full and retry idempotently after reconnect. | Evict a live record on overflow. |
| T1-T63 | T1-63 | Offline burn immediately removes the local copy, queues server/peer effects, and reports pending until confirmation. | Wait for the network before local destruction. |
| T1-T70 | T1-70 | Spike review verifies SOCKS5 subprocess/proxy decision, version constraint, and lifecycle risks. | Recommend an unexamined embedded alternative as settled. |
| T1-T71 | T1-71 | Harness proves every store route uses the Tor proxy and no request escapes the tunnel. | Bypass proxy on one route. |
| T1-T72 | T1-72 | Tor-selected plus unavailable tunnel fails closed for send and store traffic. | Permit a direct fallback. |
| T1-T73 | T1-73 | Audit record cites exit-activity finding and confirms no Vanguards toggle is exposed. | Add a Vanguards toggle. |
| T1-T74 | T1-74 | Tor fetch against a real custom-domain zone observes Onion Routing discovery and documents the `T1` WAF rule. | Not applicable: external configuration proof. |
| T1-T75 | T1-75 | Direct and Tor policies may use different constants, but each simulated stream remains constant-rate. | Make ticks adapt to link quality. |
| T1-T80 | T1-80 | Two clients complete pointer arrival → eager GET → decrypt → durable write → ACK; fetch cap stays header-only and blob negatives remain indistinguishable. | Omit fetch header, move it into URL, or restore a distinguishable negative. |
| T1-T81 | T1-81 | Captured requests/frames for 20 messages in two conversations reveal no per-conversation stable value; path has only `blob_id`, header has capability, and `P`/detect tag never appear. | Reintroduce any per-conversation request or frame constant. |
| T1-T82 | T1-82 | After delivery and local persistence, disabling the adapter still opens every delivered message; interrupted fetch sends no ACK. | Revert to lazy fetch-on-open. |
| T1-T83 | T1-83 | Independent audit reviews the merged implementation against this matrix, contract, and owner decisions. | Not applicable: independent-review verdict. |
| T1-T84 | T1-84 | Claim review allows only evidenced transport/network claims and includes D1 Time Travel, online-presence, and long-lived-circuit limits. | Add an unsupported privacy/deletion claim. |

## Cross-cutting acceptance cases

1. **Header-only authority:** `blob_id` may appear in a path, but `fetch_cap`, `ack_cap`, and
   `manage_cap` appear only in their operation-specific headers.  A capture must prove no query,
   path, frame, log fixture, or response reflects any capability.
2. **Single-negative posture:** use one fixture to compare every invalid GET and ACK state; responses
   are byte-identical 404s.  BURN is separately unconditional 204, whether absent, wrong, matching,
   already burned, or already ACKed.
3. **No premature destructive action:** a fetch-cap holder cannot burn or ACK; a decryptor cannot
   ACK until durable persistence succeeds; an interrupted fetch leaves the object retrievable.
4. **Wakeup is not delivery:** a frame for an unknown pointer causes neither a fetch nor any
   authorization attempt.  Only locally held `P` permits the normal header-authorized fetch.
5. **Offline truth:** all already persisted payloads remain readable offline; queued burn effects
   are visibly pending until server confirmation.

Cancelled or deleted tasks deliberately have no test IDs: T1-11, T1-14, T1-60, and T1-61.  Their
superseded assertions are forbidden by the revision-2 exclusion gate above.
