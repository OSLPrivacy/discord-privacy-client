# OSL known limits

Start the outside review here. Each bullet is one known limit, kept on one
physical line so it can be compared directly with proof artifacts. `TASK` names
the plan item that established or owns the limit; this is an admission list,
not a roadmap or a claim that the named task fixes the problem.

- KL-001 | TASK 3207 | No outside security review is complete: no reviewer has been appointed, no scope commissioned, and no review performed.
- KL-002 | TASK 3204 | No downloadable release or connected-service adapter has current exact-build proof; the support matrix refuses every protected-adapter public claim.
- KL-003 | TASK 3204 | The direct-message construction does not hide accounts, participants, destinations, timing, frequency, size buckets, or the presence of an encrypted block.
- KL-004 | TASK 3204 | Authentication remains classical, and key-substitution and reliable sender-attribution findings remain open.
- KL-005 | TASK 3204 | Cover text carrier is not proved: natural-language cover mode is disabled and the service receives a visible encrypted capsule.
- KL-006 | TASK 3204 | Protected text send and read is not accepted as current-release proof; its registry support is historical Discord QA.
- KL-007 | TASK 3204 | Encrypted image sending lacks named-release end-to-end transport proof.
- KL-008 | TASK 3204 | Non-image file sending is unsupported, and the known durable-plaintext staging finding remains open.
- KL-009 | TASK 3204 | Group and server-channel protection is disabled; direct-message scope is the only construction used by the shipping product.
- KL-010 | TASK 3204 | Old-message protection is unavailable because the live path is stateless and the ratchet runtime is disabled.
- KL-011 | TASK 3204 | Attachment Privacy Guard is unwired because the metadata-stripping seam has no production caller.
- KL-012 | TASK 3204 | Before-send warning is unwired because the detection code has no shipping-app caller.
- KL-013 | TASK 3204 | Timed expiry is unwired and has no current cryptographic-erasure path.
- KL-014 | TASK 3204 | View once has incomplete production wiring and incomplete two-party consent proof.
- KL-015 | TASK 3204 | Burn is not proved as a peer action and cannot promise un-send, provider deletion, screenshot recall, or destruction of every decryption capability.
- KL-016 | TASK 3204 | Link protection has no recorded implementation.
- KL-017 | TASK 3204 | Scrub discovery does not qualify provider identity, inventory, media, or completeness end to end.
- KL-018 | TASK 3204 | Guided deletion handoff is not connected end to end and depends on unqualified Scrub discovery.
- KL-019 | TASK 3204 | AutoScrub has only switched-off interface scaffolding and is not a working engine.
- KL-020 | TASK 3204 | AI-generated carrier text is not offered and cloud generation would not be end-to-end private.
- KL-021 | TASK 3204 | Processing credits are not implemented as a purchasable capability and are not on sale.
- KL-022 | TASK 3204 | Automatic Pro expiry is not implemented while checkout is paused.
- KL-023 | TASK 3204 | Country exposure comparison is a static sourced illustration, not a device scan or live protection measurement.
- KL-024 | TASK 3204 | Website Scrub is a username-only illustration and does not inspect browser or local-account data.
- KL-025 | TASK 3204 | Product animations are drawings of intended behavior, not recordings or proof of a build.
- KL-026 | TASK 3204 | Both public prototypes are simulations and do not prove real account access, encryption, scanning, sending, or deletion.
- KL-027 | TASK 3204 | Six of seven separately audited Hub Home statements are unsupported; only the prototype no-network-request statement is supported, and only for the prototype.
- KL-028 | TASK 3204 | OSL does not protect against screen photos, hardware capture, screenshots, copies, exports, recordings, or reports.
- KL-029 | TASK 3204 | OSL does not protect against malware, modified or compromised endpoints, unlocked devices, or theft of recipient long-term secret keys.
- KL-030 | TASK 3204 | OSL does not protect against provider-account risk including metadata observation, rule or UI changes, suspension, bans, retained copies, exports, or backups.
- KL-031 | TASK 3204 | OSL does not protect against a malicious or cooperating recipient reading, retaining, forwarding, or republishing plaintext.
- KL-032 | TASK 3204 | OSL does not protect against targeted investigation, traffic analysis, identity or account correlation, destination discovery, timing, frequency, or size leakage.
- KL-033 | TASK 3204 | OSL cannot delete recipient copies, provider history, exports, backups, or already opened copies.
- KL-034 | TASK 3401 | The recorded Windows measurement is only a refusal observation and does not prove the new front-window grab works.
- KL-035 | TASK 3300 | The service-limit research does not prove timed delete works in OSL.
- KL-036 | TASK 1089d | Live WhatsApp Status composer behavior remains unproven in this run because the configured QA VM could not be opened.

## How this list stays honest

Run `python3 scripts/task3206_known_limits.py --proof-dir proof --list`. The
command validates every entry, prints the list and count, and recursively checks
that every physical line in the proof folder containing `still unproven`
(case-insensitive, with arbitrary whitespace) is reproduced on a task-numbered
line above. `--self-test` also proves the check rejects an unlisted proof line,
a line without a task number, and deletion of a required known limit.
