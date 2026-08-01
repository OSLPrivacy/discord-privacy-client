# Hosted Scrub preload handoff (T4)

Status: implementation handoff. This document does not install a preload or change
`service_host.rs`. T4 owns that integration.

## Purpose and boundary

Scrub may drive the already visible, signed-in UI only after the host has installed a
provider-specific preload for that exact hosted webview. The preload is a narrow,
delete-only capability, not a general automation bridge. It implements the four
semantic commands defined by
`apps/osl-hub-ui/src/scrub-hosted-session-channel.ts`:

| Command | Required meaning |
| --- | --- |
| `scrollHistory` | Load a bounded amount of earlier provider UI history. |
| `listOwnItems` | Return only items proved to be authored by the signed-in account. |
| `deleteOwnItem` | Request the provider's fixed visible delete action for one previously listed own item. |
| `verifyGone` | Perform provider-UI readback and report both coverage and absence. |

The checked UI-side envelope in `scrub-hosted-session-port.ts` remains the protocol
authority. T4 must carry commands and replies without adding a generic operation,
selector, script, or page-originated IPC route.

## T4 implementation task

Implement the following in the hosted-webview construction and lifecycle code owned
by T4. This is the prerequisite for hosted Scrub adapters; until all acceptance
conditions below pass against a real session, `liveConfirmed` remains false.

1. **Exactly one preload for an allowlisted webview.** At hosted-webview creation,
   select one compiled-in provider preload only when the service and final origin are
   allowlisted for Scrub. Do not install it in OSL's main webview, borrowed browser,
   arbitrary child webview, or an unrecognized/redirected origin. A second install
   for the same live webview is an error; it must not create a second channel or
   replace an extant binding.

2. **Bind every request to the proven host identity.** The native channel binding is
   the exact `ActiveServiceHost` identity: `service_id`, `account_id`, and
   `generation` (with its owner namespace held by the host). At dispatch, compare
   the stored binding with the currently active host identity. A mismatch, missing
   host, or stale generation rejects the request without reaching the preload.
   Replies must retain the preload's account and session epoch so the checked port
   can reject an account/session mismatch.

3. **Accept only the tagged delete-only protocol.** Decode only
   `scrollHistory`, `listOwnItems`, `deleteOwnItem`, and `verifyGone`, with their
   existing bounded request/reply envelopes. Reject unknown tags, malformed values,
   unowned IDs, and replies that do not match the binding. The host must not expose
   evaluation, arbitrary JavaScript, selectors, click/input events, network access,
   or any send/post/react/join/upload capability.

4. **Invalidate before lifecycle state can be reused.** Destroy the channel and its
   binding before or atomically with navigation, suspension, account mutation,
   generation change, close, and application shutdown. A request that races an
   invalidation fails closed. Reopening or returning to an allowed origin creates a
   new webview generation and requires a fresh preload, binding, and live probe;
   it must never revive the prior channel.

5. **Keep the page untrusted.** The provider page must not be able to call native
   commands, choose a provider recipe, choose selectors, provide executable text,
   or receive a native IPC handle. The native side initiates each tagged request to
   the isolated preload and verifies the binding on both send and reply. Provider
   recipes and selectors stay compiled into the provider preload.

6. **Prove liveness before capability advertisement.** Do not advertise
   `liveConfirmed` merely because a preload was compiled or attached. For the
   current binding, successfully complete a provider list probe and an independent
   provider-UI readback probe, validate both replies through the checked envelope,
   and confirm the returned account/session identity. If there is no safe readback
   target, a reply is ambiguous, or any lifecycle event occurs, leave the capability
   unavailable. The probe result is scoped to the binding and expires with it.

## T4 acceptance tests

These are behavior tests for T4's host integration; they must use a fake webview and
preload transport rather than inspect Rust source text.

1. Opening an allowlisted Gmail, Discord, or Telegram webview installs one matching
   preload; an unknown origin and a borrowed browser install none; a duplicate
   install is refused.
2. A command bound to one account or generation cannot dispatch after switching the
   account, navigating, suspending, closing, shutting down, or reopening the host.
   In each case the fake preload observes no command.
3. Each of the four tagged commands round-trips a checked envelope. An unknown tag,
   malformed envelope, foreign account/session, or unowned item is rejected before
   Scrub receives a result.
4. No page-originated message can obtain a native channel or execute a selector,
   script, input event, network request, or messaging action.
5. `liveConfirmed` is false after attachment and becomes true only after a successful
   list-plus-readback probe for the current binding. Invalidating that binding makes
   it false immediately and requires a new successful probe after reopening.

## Ownership and escalation

T12 owns the delete-only protocol and this handoff; T4 owns the preload installation
and `service_host.rs` lifecycle integration. D55 permits this consent-gated,
visible-UI path but does not widen the authority boundary. This handoff is on the
critical path: hosted deletion work cannot claim a live path until T4 lands the
integration and its acceptance tests. Escalate to the owner if T4 cannot schedule it.
