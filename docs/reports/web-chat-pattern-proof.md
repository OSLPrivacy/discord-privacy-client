# Web chat pattern proof

Status: **not proven; fail closed** (2026-08-02).

The shared fixed-origin adapter and its C1-C10 web conformance fixture exist,
but this authoring host has neither a Windows VM nor a signed-in X account.
The five observations required for the X proof therefore cannot be made
honestly here. No X, Instagram, Snapchat, or Messenger L2/L3 profile is
granted from this record.

## T4-P1 — locate

Required evidence is a signed-in X DM and a WebView2 UIA/MSAA tree showing the
composer and transcript. The Linux host cannot enumerate either surface. The
required accessibility-wake operation is exercised by the fixture gate, but
that is not live X evidence.

## T4-P2 — read state

Not measured. Virtualised transcript bounds and `read_was_complete` must be
observed in a live signed-in X conversation; a fixture cannot establish them.

## T4-P3 — destination

Not measured. Conversation-switch scope bindings must be captured from a live
account. No permissive mapping for `Unknown` is installed.

## T4-P4 — placement and commit

Not measured. This also requires T3-F4's isolated-VM input attestation. No
web adapter invokes input or exposes send capability from this host.

## T4-P5 — paint targets

Not measured. The shared conformance fixture rejects Pixel Exact targets and
Exact targets without a carrier digest, but it is not evidence of an X row.

## Required Windows re-run

On an isolated Windows VM, sign in to X, run the shipping binary's `WEB-W3`
suite against a DM, capture the P1–P5 observations, and replace this status
with the evidence and binary hash. Until then `CHAT-PATTERN` is unavailable.
