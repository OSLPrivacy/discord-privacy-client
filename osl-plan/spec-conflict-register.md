# Specification conflict register

This is the live closure record for the spec-vs-plan reconciliation, not a
template. The master at `docs/design/osl-master-decision-2026-07-26.md` is
product authority. The reconciliation audit found 17 contradictions; this
register holds the material owner choices and points to the source record for
the remaining non-material closures.

| Conflict ID | Claim A / authority | Claim B / authority | Affected artifacts | Resolution | Owner escalation | Status |
| --- | --- | --- | --- | --- | --- | --- |
| SCR-001 | Master §0.5: AutoScrub UNATTENDED (D85) | Older master wording: attended | scrub guide, checklist, public copy | D85 controls; all current references use UNATTENDED. | D85, recorded owner decision | closed |
| SCR-002 | Master lines 591-598: Notes/Creative minimum in v1 | D43: no Notes build track | master §0.5, T2/T7 status, internal checklist, public copy | D109 defers Notes/Creative for v1; only honest `Coming soon` is permitted. | OD-1 → D109 (2026-08-02) | closed |
| SCR-003 | Master lines 555-563: verified website/phone Scrub | D75: website deferred/static alternative | master §0.5, T11-F1–F5, website checklist, public claim eligibility | D91 requires a live verified sweep; static/mock presentation cannot satisfy it. | OD-2 → D91 (2026-08-02) | closed |
| SCR-004 | Master line 799: Scheme-1 requires caller, ordered migrations, Worker and live proof | identity OQ-4/T5 posture deferred `osl1_` | master §0.5, identity contract, T5 DAG/checklist, keyserver release claim | D110 orders the bounded Scheme-1 rollout; `osl_` remains available and no Scheme-1 claim moves before proof. | OD-3 → D110 (2026-08-02) | closed |
| SCR-005 | Sender-key forward secrecy claim | Existing durable mirror needed for group delivery | master §0.5, crypto contract, onboarding, sender-key tests, public security copy | D111 makes the trade-off an explicit, unselected onboarding choice and requires the session-only store for the protective choice. | OD-4 → D111 (2026-08-02) | closed |

The detailed escalation evidence is in
`/home/liamw/osl-plan/OWNER-DECISIONS-NEEDED.md`; the authoritative resolutions
are D91 and D109–D112 in `/home/liamw/osl-plan/plan/09-DECISIONS.md`. Each
closure lists the artefacts that must be updated or explicitly superseded;
runtime evidence may establish status only and never rewrites intent.

Authority order is: dated owner product decision, numbered master decision,
approved architecture contract, current acceptance/checklist,
implementation/runtime evidence, then working notes or memory. Every semantic
master edit increments the revision and adds a one-line §0.5 delta. Record the
first applicable authority for planned work. A material privacy or security
choice must be escalated to the owner, even when a lower-ranked source suggests
a convenient alternative.
