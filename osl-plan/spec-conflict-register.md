# Specification conflict register

| Conflict ID | Claim A / authority | Claim B / authority | Affected artifacts | Resolution | Owner escalation | Status |
| --- | --- | --- | --- | --- | --- | --- |
| SCR-001 | Master §0.5: AutoScrub UNATTENDED (D85) | Older master wording: attended | scrub guide, checklist, public copy | D85 is controlling; update all references to UNATTENDED | Decision already recorded as D85 | closed |

Authority order is: dated owner product decision, numbered master decision, approved architecture
contract, current acceptance/checklist, implementation/runtime evidence, then working notes or
memory. Runtime evidence can establish implementation status; it cannot rewrite product intent.
Every semantic master edit increments the revision and adds a one-line §0.5 delta. Record the first
applicable authority for planned work. A material privacy or security choice must be escalated to the
owner, even when a lower-ranked source suggests a convenient alternative. Close a conflict only when
every listed artifact has been updated or deliberately superseded.
