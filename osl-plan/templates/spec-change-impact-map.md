# Owner specification / architecture change impact map

Use this packet before implementation. Attach the dated owner wording verbatim, list each affected
stable feature ID, and identify every previous claim that it supersedes. A diagnosis, implementation
suggestion, old memory, or preference is not owner authority.

| Area | Required decision and acceptance evidence |
| --- | --- |
| Product authority | Dated owner wording, affected IDs, first applicable authority, supersession map |
| Contracts | Contract-first acceptance, defaults, versioning, and wire/crypto consequences |
| UI and native | Screen states, native integration, consent and accessibility impact |
| Persistence | Migration, old-data handling, downgrade resistance, removal date |
| Network and deploy | Protocol/deployment compatibility, rollout and observability |
| Pricing and tier | Tier eligibility, public wording, and billing/consent changes |
| Quality | Test matrix, documentation/site copy, telemetry, rollback and current tasks |
| Plan | Recalculated DAG, weights, denominator, critical path, ETA, merge sequence; synchronized views |

Before implementation, write the compatibility plan: transition behavior, downgrade resistance,
rollback owner, old-data treatment, and removal date. Send the owner a plain-language notice naming
what changes, what remains compatible, and the rollback path.
