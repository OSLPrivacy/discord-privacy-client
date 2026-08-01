# OSL Chat transport divergence

**Status:** open — raised against T1 on 2026-07-31. This is an evidence record,
not a transport design or an implementation plan.

## Contract boundary

`plan/03-CONTRACTS/transport.md` §0 defines its protected-message transport as
the only way a protected message or attachment leaves or enters the application.
It explicitly excludes the signed `keyserver-cf` control-inbox lane because that
lane is addressed by `osl_user_id`.

OSL Chat currently uses that excluded lane. Its relay posts and reads
`/v1/control-inbox`; its message material is stored through
`/v1/wrapped-keys`. The resulting path is outside the transport contract rather
than an implementation of it.

| current OSL Chat fact | evidence | contract conflict |
|---|---|---|
| Relay notices are delivered through `/v1/control-inbox`. | `apps/osl-hub/src/broker.rs`: `post_control_inbox` at 4800 and `fetch_peer_control_inbox` at 3818–3824. | `transport.md` §0 excludes this control-inbox lane. |
| Message material uses `/v1/wrapped-keys` before a relay notice is posted. | `apps/osl-hub/src/broker.rs`: the B49 contract test captures `POST /v1/wrapped-keys` at 10386–10396 before `POST /v1/control-inbox`. | This is the excluded OSL Chat delivery path, not the §0 blob/attachment transport. |
| A relay scope is deterministic for a conversation. | `native_overlay_relay_scope_id` at `apps/osl-hub/src/broker.rs:5241–5249` returns `native-overlay:{conversation_binding}`. | Violates P3: nothing in or alongside a pointer may remain stable across two messages in one conversation. |
| The receiving lane is signed and addressed by `osl_user_id`. | `transport.md` §0 identifies control-inbox as signed and `osl_user_id`-addressed; broker calls supply `manual.peer_osl_user_id` to the control-inbox client at 4800 and 3823. | Violates D3's no-authentication-on-fetch requirement: possession, not an identity, must authorize fetch. |

## T1 coverage consequence

T1 must not close its transport contract claiming OSL Chat coverage while this
path remains. OSL Chat is a known divergence: it has not adopted the
pointer-only blob/attachment lanes, the per-message-unlinkability requirement
(P3), or D3's possession-only fetch authorization.

This record may be retired only with evidence that the OSL Chat path no longer
uses the signed, `osl_user_id`-addressed control-inbox delivery lane and no
longer derives a stable relay identifier from a conversation binding.
