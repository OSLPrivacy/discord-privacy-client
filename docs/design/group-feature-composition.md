# Group feature composition contract input (T18-D1)

This is ready-to-paste wording for T2. It does not amend `features.md`.

## §6.1a — Group destruction acknowledgements

For a group, destruction acknowledgement state is tracked independently for
every recipient device. The aggregate is `Confirmed` only when every required
device confirmation has arrived. A missing member or offline device is
`Unconfirmed`; absence is never interpreted as compliance or refusal.

## §8.11a — Group burn with offline members

A group burn performs one server deletion and one deletion instruction per
recipient device. Server deletion and each recipient state are displayed
independently. An offline recipient remains queued and `Unconfirmed`; no UI
may collapse these states to a single green tick.

## §8.3 / §10.3 — Group view-once

View-once is one independently single-fetch blob per `(message, recipient
device)`. A successful or failed fetch for one member cannot consume, destroy,
or change the state of another member's blob. The sender sees the independent
states and must not describe the group as having one shared fetch.

## §8.9 — Group expiry under clock skew

Each recipient applies the existing two-clock expiry calculation locally. For
the same terminal facts, every group recipient uses R2's fixed severity order
to select the displayed reason; arrival ordering must not change that reason.

§7 R1–R6 and the §4.3 CLAIM model already generalise to N recipients and are
unchanged by this extension.
