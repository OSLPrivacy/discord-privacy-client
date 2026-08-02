# Enclave visibility table

**Status: NOT BUILT.** This is a design boundary for OSL Enclaves, not product
copy and not a claim that any Enclave capability currently ships. The channel-bearing
container is named *Enclave* by owner decision D84; earlier “Spaces” and “Circles”
names are superseded for this product.

An admin is a member with authority to propose signed governance actions. That
authority must not grant an admin a private surveillance view. Unless a row says
otherwise, the member and admin columns are deliberately identical.

| Visibility subject | Member | Admin | Relay |
|---|---|---|---|
| Message content | Can decrypt only content for channels their local keys authorize. Cannot read another channel merely because of their role. **NOT BUILT.** | Can decrypt only content for channels their local keys authorize. Cannot read another channel merely because of their role. **NOT BUILT.** | Carries opaque encrypted blobs; cannot decrypt message content. **NOT BUILT.** |
| Roster | Can see the current locally held roster needed to participate. It is not an online-member list. **NOT BUILT.** | Can see the current locally held roster needed to participate. It is not an online-member list. **NOT BUILT.** | Must not hold or expose an Enclave roster. **NOT BUILT.** |
| Roles | Can see role assignments conveyed in the member-visible governance state. Roles do not disclose member activity. **NOT BUILT.** | Can see role assignments conveyed in the member-visible governance state. Roles do not disclose member activity. **NOT BUILT.** | Cannot read role assignments. **NOT BUILT.** |
| Join and leave | Can see signed membership changes in the member-visible governance state, but no pre-join message history. **NOT BUILT.** | Can see signed membership changes in the member-visible governance state, but no pre-join message history. **NOT BUILT.** | Must not learn membership from a server-held roster; delivery fan-out must not become a roster API. **NOT BUILT.** |
| Presence | Receives no online, last-seen, typing, or read-receipt signal from Enclave membership. **NOT BUILT.** | Receives no online, last-seen, typing, or read-receipt signal from Enclave membership. **NOT BUILT.** | Connected delivery can reveal that a device is connected; polling reveals requests at the polling cadence. The onboarding choice must state this trade-off (D64). **NOT BUILT.** |
| Timing | Sees messages delivered to their own device and local receive time; cannot use Enclave presence or receipts to infer another member’s activity. **NOT BUILT.** | Sees messages delivered to their own device and local receive time; cannot use Enclave presence or receipts to infer another member’s activity. **NOT BUILT.** | Observes transport events such as connections, uploads, fetches, and their timing. Fixed-rate connected frames reduce, but do not erase, this metadata. **NOT BUILT.** |
| Size | Sees plaintext size for content their device decrypts and local storage use. **NOT BUILT.** | Sees plaintext size for content their device decrypts and local storage use. **NOT BUILT.** | Sees encrypted object and frame sizes after required padding, plus the number of objects it relays. **NOT BUILT.** |
| Channel names | Sees names only for channels represented in locally held Enclave state. **NOT BUILT.** | Sees names only for channels represented in locally held Enclave state. **NOT BUILT.** | Cannot read channel names. **NOT BUILT.** |
| Governance events | Sees signed governance events—membership, role, and key-rotation changes—rather than a log of what members said or read. **NOT BUILT.** | Sees signed governance events—membership, role, and key-rotation changes—rather than a log of what members said or read. **NOT BUILT.** | Carries opaque governance-event ciphertext and necessary delivery metadata; it cannot read the event. **NOT BUILT.** |

## Evidence and implementation rule

Every row above is marked **NOT BUILT** because the required Enclave contract and
implementation do not yet exist in this checkout. When a row becomes implemented,
replace that marker with a precise code location or frozen contract clause and retain
the limit stated in the row. Do not replace a relay limitation with a blanket
“metadata-free” claim: D64 explicitly preserves a connected-versus-polling presence
trade-off.

The executable gate at
`apps/osl-hub-ui/src/osl-enclaves-visibility.test.ts` requires all nine subjects,
equal member/admin visibility cells, and either implementation evidence or the
`NOT BUILT` marker for every table row.
