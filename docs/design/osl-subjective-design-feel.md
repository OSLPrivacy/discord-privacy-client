# OSL subjective design feel

This document freezes the product-feel rules that should survive visual redesigns,
connector changes and implementation swaps. It is intentionally user-facing: it
defines what OSL must feel like before it defines how OSL is built.

## Complexity belongs behind the product

OSL presents itself as one calm privacy app for accounts, conversations and local
cleanup. The user-facing model is protection state, trusted people, connected
accounts, private conversations, cleanup actions and activity history. Engineering
concepts such as keyservers, ratchets, receipts, browser profiles, or provider adapters
belong behind those product nouns. They must not appear as navigation,
onboarding choices, warning labels, settings names, status labels or other
user-facing concepts.

When an implementation detail affects a decision the user must make, the interface
translates it into a plain consequence and a safe action. For example, OSL can say a
conversation is not ready for protected send, that a cleanup action can only be
assisted, or that a result is unknown. It should not ask the user to reason about
protocol names, storage layouts, automation internals or service-specific plumbing.

Advanced diagnostics may include machine-readable implementation fields for support
or audit exports, but visible UI labels still translate them into the product model.
Diagnostics must never substitute for a clear product answer on the main screen.
