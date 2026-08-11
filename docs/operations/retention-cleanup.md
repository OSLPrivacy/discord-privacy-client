# Unattended retention cleanup

When retention expires, OSL schedules deletion automatically and retries failed cleanup without waiting for a person. OSL does not promise a staffed response time.

The scheduler rediscovers due objects from the authoritative inventory on every
run. A durable per-object lease lets another Worker resume after an expired
lease, and transient provider failures retain the object and retry state. Only
three repeated observations of the same explicitly unrecoverable provider
reason make cleanup terminal. Terminal reports use the existing Telegram owner
report destination and include `policy`, `object`, `oldest_due_item`,
`attempts`, and `terminal_reason`; delivery does not require acknowledgement.
