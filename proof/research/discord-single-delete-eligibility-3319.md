# Discord single-message deletion eligibility — TASK 3319

Frozen: 2026-08-13

Independent carrier source: <https://docs.discord.com/developers/resources/message>

Operation: `Delete Message` (one channel/message target). The current official
reference specifies a successful `204` response and permission conditions, but
publishes no finite message-age eligibility deadline for that operation. Its
separate `Bulk Delete Messages` operation explicitly has a two-week age limit;
that is not the shipping operation because it requires 2–100 identifiers and
would violate the exact-one-target contract.

Result: `finiteDeadline=none`; source id is
`discord-message-resource-single-delete-2026-08-13`.

The live Windows boundary probe is
`scripts/qa/task-3319-discord-delete-boundary-probe.ps1`. It uses the installed
Discord process plus Windows PowerShell, `tasklist.exe`, and `CopyFromScreen`,
and refuses a capture with fewer than 32 distinct RGB colours. It intentionally
does not delete any user message: a throwaway-account execution is required to
produce a live carrier receipt.
