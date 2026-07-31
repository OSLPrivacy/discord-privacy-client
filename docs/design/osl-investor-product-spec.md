# OSL product brief

> Internal investor and partner explanation. This describes the complete product OSL is building
> in plain English. It is not approved public website copy, and it does not imply that every
> feature is available today. The technical authority remains
> [`osl-master-decision-2026-07-26.md`](osl-master-decision-2026-07-26.md). Current evidence lives in
> [`osl-internal-build-checklist.md`](osl-internal-build-checklist.md).

## The short version

OSL is a desktop privacy hub for the accounts and apps people already use.

Most privacy products ask users to abandon their existing network and persuade everyone they know
to move somewhere new. OSL takes a different route. It adds a private layer around services such as
Discord, Signal, WhatsApp, Telegram, email, and the web.

The connected service still carries the traffic and provides its normal interface. OSL keeps the
sensitive part under the user's control:

- the real message or file;
- the keys that protect it;
- the identity of the person who sent it;
- the user's trust decisions;
- expiry, view-once, and cleanup rules;
- proof of what was sent, removed, or left uncertain.

OSL is one product with three related jobs:

1. protect communication without requiring a new social network;
2. find and reduce the user's existing digital footprint;
3. provide a private local workspace for notes and files.

## The problem OSL solves

People have years of relationships, conversations, files, and account history spread across
services they do not control. Moving to a private messenger rarely solves this because their
friends, work, and communities stay on the original platforms.

The result is an awkward choice:

- keep using the services where everyone already is and accept their privacy model; or
- move to a safer service and lose the network that made the original service useful.

OSL is designed to remove that choice. A user can keep the account and interface they already know
while OSL protects selected content locally.

The second problem is historical. Even if a user becomes more careful today, years of old posts,
messages, accounts, and public records may remain scattered across the internet. OSL's Scrub tools
help the user find that material, review it, remove what can safely be removed, and receive an
honest report about what could not be verified.

## The product

The main product is OSL Hub, a Windows desktop app. It recognizes supported apps and accounts on
the computer, then gives the user one place to control protection, identity, cleanup, and status.

OSL does not silently take over an account. The user chooses the app, account, and action. If OSL
cannot prove that it is looking at the right place, it refuses or asks the user to finish the step
manually.

### Protected messaging

A protected message follows a simple model:

1. The user types the real message into OSL.
2. OSL encrypts the message on the user's computer.
3. OSL creates harmless replacement content called a carrier.
4. The connected service sends the carrier as an ordinary message.
5. Another OSL user recognizes the carrier, verifies the sender, retrieves the encrypted content,
   and displays the real message in the correct place.

The service sees the carrier, not the original message. The carrier is camouflage for transport.
It is not the encryption itself.

The same model can support text, pictures, video, and files. For media, OSL must also match the
space occupied by the original item so the protected version fits naturally in the connected app.

### A clear privacy switch

OSL separates two decisions:

- Lock controls whether protected sending is active.
- Eye controls whether decrypted content is visible.

When protection is off, the user is using the connected service normally. When Eye is off, the
service's harmless carrier remains visible. OSL only reveals protected content after it verifies
the message, account, conversation, and sender.

This distinction matters because users should never have to guess whether they are typing
privately.

### Sending modes

Different users want different levels of automation. OSL supports three intended workflows:

- The default copies the carrier to the clipboard. The user pastes and sends it.
- Double Enter places the carrier first, then waits for a second Enter before sending.
- Single Enter places and sends only after OSL proves it has the right conversation and the full
  carrier arrived.

Every send ends in one of three states: sent, not sent, or uncertain. OSL does not automatically
retry an uncertain send because the first attempt may already have arrived.

### Identity and trust

Encryption is only useful if the app knows whose keys it is using. OSL therefore treats identity as
a product feature, not a hidden technical detail.

The finished identity system includes:

- a local OSL identity protected by a password and recovery method;
- proof that all encryption and signing keys belong to the same identity;
- proof that a service account belongs to the person registering it;
- friends and safety numbers for high-confidence verification;
- clear warnings when a trusted key changes;
- scoped trust so one approval does not silently authorize every account or device;
- downgrade protection so a contact cannot silently fall back to weaker protection.

OSL must show the authenticated sender, not a name supplied by the message itself.

### Stronger conversation security

The long-term messaging design adds forward secrecy and recovery after compromise. In plain
English, stealing today's key should not reveal the entire past, and a conversation should be able
to become safe again after an old key leaks.

Those properties require a reviewed ratchet, durable state, replay protection, safe crash
recovery, and a controlled two-person test. OSL does not claim those properties merely because the
cryptographic code exists.

### Protected media and message lifecycle

The complete product includes:

- encrypted pictures, videos, and files;
- view-once text and media;
- timed expiry;
- open, read, and deletion receipts;
- local and server cleanup;
- peer cleanup requests;
- provider deletion requests;
- retryable cleanup when a device or provider is temporarily unavailable.

OSL calls the cleanup feature Burn. Burn removes copies that OSL or a cooperative service can
reach. It does not reach into another person's device, undo a screenshot, stop a camera, or make a
key they already possess disappear.

That limit is part of the product. OSL reports which cleanup steps succeeded instead of presenting
one animation as proof that every copy vanished.

### Screenshot and sensitive-content protections

OSL can use operating-system capture protection for sensitive views. If it cannot prove the
protection was active before the first sensitive pixel appeared, it reveals nothing.

The product can also warn when a user is about to send likely sensitive material through an
unprotected message box. This is a consequence warning, not a moderation system. It can cover
credentials, explicit material, graphic violence, slurs, and business secrets without treating
ordinary political discussion as inherently unsafe.

## Scrub

Scrub helps users reduce the material already attached to their online identities.

The desktop app can discover supported accounts and local evidence, then guide the user through a
fixed safety flow:

1. choose an account and category;
2. scan;
3. review the findings;
4. confirm an exact plan;
5. attempt removal;
6. verify the result;
7. produce a receipt that distinguishes success, failure, and uncertainty.

The user remains in control of attended deletion. OSL does not infer account ownership from a
username or delete an ambiguous target.

### Free Scrub

Free Scrub is open source and attended. It helps the user find material, review it, and carry out
safe actions while the app is open.

The website can offer a smaller username-only scan of public sources. It does not receive the
user's browser profile, local account inventory, or private service data.

### Pro AutoScrub

AutoScrub repeats a plan the user already reviewed. It is designed for routine cleanup without
turning OSL into an unattended account-control bot.

The product includes:

- strict limits on targets and action counts;
- visible status and receipts;
- pause and Stop controls;
- account and schema checks before every action;
- provider-specific safety rules;
- refusal when the page, account, or result no longer matches the approved plan.

An optional narrow module may contain the minimum proprietary work needed to operate AutoScrub. It
is installed separately with consent. Encryption, identity, storage, receipts, and Free Scrub
remain open source.

OSL may later offer cloud AutoScrub for users who do not want a computer running. That is a more
sensitive service because temporary credentials and account information leave the device. It
requires separate consent, isolation, short retention, revocation, wiping, and a deletion receipt.

## Notes and private files

OSL Notes is the local workspace side of the product. Its first useful version is intentionally
small:

- encrypted local notes;
- links, search, and basic organization;
- encrypted file storage and sharing;
- safe import and export;
- a focused set of reliable editing tools.

Larger office, art, media, 3D, plugin, and collaboration tools belong to the longer product vision.
They do not need to block the first release and should remain unavailable until they have real
production paths.

## Connected apps

Discord is the reference integration. It proves the complete model before OSL claims broad
coverage.

Other intended connectors include:

- Signal;
- WhatsApp;
- Telegram, if its desktop app exposes enough information for safe operation;
- email and IMAP accounts;
- browser history and account discovery;
- hosted services that can be operated through a tightly controlled session.

Each connector has its own signed, versioned adapter profile. The profile can describe closed,
typed selectors and fallback strategies. It cannot contain arbitrary scripts or shell commands.
OSL verifies the app, service, account, profile revision, signature, expiry, and harmless canary
before using it. Ambiguity causes refusal.

An app is either supported, experimental, coming soon, or externally blocked. OSL does not label an
app supported because a prototype or test fixture exists.

## Privacy model

OSL is built around local authority:

- private keys stay under the user's control;
- real message content is encrypted before it reaches the connected service;
- sensitive local records use authenticated encryption;
- destructive actions require an exact target and recoverable state;
- uncertain external actions are not blindly repeated;
- logs and receipts minimize identities and raw content;
- the app refuses when it cannot bind evidence to the correct account, process, window, build, or
  action.

Local does not mean invisible. File sizes, timing, counts, equality patterns, or the existence of a
conversation may still leak unless a specific boundary proves otherwise. OSL describes those
limits directly.

## Free, Pro, and revenue

Basic encryption remains available without a subscription.

The intended Pro model is a $5 prepaid month:

- the customer buys a code and redeems it in the app;
- the month starts at redemption;
- nothing renews automatically;
- OSL does not keep payment details;
- the customer does not need an OSL account;
- expiry removes Pro features without touching keys, messages, notes, or basic encryption.

Processing credits are separate one-time purchases for optional cloud AI work. Running out of
credits never disables encryption.

Pro can include:

- AI-generated carrier text;
- optional cloud processing;
- advanced protected media;
- AutoScrub;
- other clearly identified convenience features.

Card, Bitcoin, and Monero payments follow the same prepaid model.

The live licensing system must match this description before the company advertises the redemption
clock publicly.

## Why the model can scale

OSL does not need to replace every network. A new connector adds support for an existing service
while sharing the same identity, encryption, receipts, safety controls, and account inventory.

The adapter system is deliberately narrow. A connector supplies service-specific observations and
actions, while the central product retains authority over trust, confirmation, limits, and proof.
This lets OSL add coverage without handing arbitrary code the keys to the product.

The same local account graph also supports Scrub, protected messaging, safety warnings, and future
private workspace features. A connector can therefore improve several parts of the product instead
of serving one isolated tool.

## What makes OSL different

OSL combines ideas that are usually sold as separate products:

- private communication without abandoning existing communities;
- digital-footprint discovery and cleanup;
- a local encrypted workspace;
- one account and trust map across supported services;
- evidence that distinguishes "the code ran" from "the action actually happened."

The important difference is not a claim that OSL can control another company's service. It is the
opposite. OSL is designed to know where its authority ends and to show the user when a service,
device, or other person did not cooperate.

## What a finished release means

A release is not finished when a feature compiles or passes a unit test. OSL requires:

- the exact distributable build;
- a test that fails when the feature is broken;
- real Windows and connected-app evidence where the feature depends on them;
- two controlled identities for two-person behavior;
- negative tests for security and destructive actions;
- receipts bound to the exact account, process, build, target, and result;
- public claims that match the evidence for the released binary;
- a signed, reproducible, rollback-capable release path.

The public website is a finished-product surface. It lists only features that are independently
verified and available in the release. Internal progress, unfinished-feature scores, and roadmap
claims stay out of public sales copy.

## Current maturity

OSL is a working but incomplete product under active integration.

The current codebase already contains substantial identity, encryption, Discord overlay, account
discovery, Scrub, storage, website, and release infrastructure. Some of the strongest subsystems
are still unwired, some evidence is source or test only, and several connected-app features still
need exact-build Windows or two-person proof.

The near-term release strategy is:

1. ship a stable, honest Discord protected-text experience;
2. prove two controlled identities can send, receive, restart, and recover safely;
3. ship a demonstrable attended Scrub flow with Stop and verified results;
4. qualify only the connectors that pass their real boundaries;
5. publish one clean, signed release whose website claims match the binary.

Everything else remains in the product plan, but unfinished breadth does not block an honest first
release.

## One-sentence investor description

OSL is a desktop privacy layer that protects communication, reduces old digital exposure, and keeps
private work local while letting people continue using the online services and communities they
already have.
