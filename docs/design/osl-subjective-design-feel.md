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

## Frozen user-facing contract

Every user-facing surface must preserve the same mental model even when the
implementation changes underneath it:

- The app speaks in protection state, trusted people, connected accounts, private
  conversations, cleanup actions and activity history.
- The app refuses to expose implementation machinery as a decision the user must
  understand before acting.
- When protection cannot proceed, the app gives a plain consequence and the next
  safe action, including an explicit unknown state when certainty is unavailable.
- Advanced exports may carry implementation fields for support, but the main UI
  remains accountable for the product answer in ordinary language.

This contract is user-facing behavior, not copy decoration. A design that teaches
the user protocol, storage, automation, transport or service-plumbing concepts in
order to operate OSL violates the design feel even if the underlying feature works.

## Freeze the user-facing complexity-hiding product contract

This acceptance test passes only when a proposed user-facing surface preserves the
contract above:

1. A screen, onboarding step, warning, settings row, status label or navigation
   item passes when its visible choices are product nouns: protection state,
   trusted people, connected accounts, private conversations, cleanup actions and
   activity history.
2. The same surface fails when the user must choose, diagnose or trust OSL by
   reading implementation nouns such as protocol state, storage layout,
   automation internals, transport plumbing or service-adapter mechanics.
3. A refusal passes when it gives a plain consequence and a safe action, such as
   "protected send is not ready for this conversation" or "cleanup can only be
   assisted here."
4. A refusal fails when it exposes the internal mechanism as the answer, even if
   the internal detail is accurate.
5. Advanced support exports pass when implementation fields are machine-readable
   and secondary; they fail when those fields replace the ordinary-language answer
   on the main screen.

Mentally invert the behavior before accepting a design: if replacing the product
nouns with implementation machinery would still pass review, the test is
vacuous and the design does not satisfy this contract.

The same document-level test keeps implementation machinery behind the product
model in concrete candidate surfaces:

| Candidate surface | Expected verdict | Product behavior under test |
|---|---|---|
| Main navigation item named `Private conversations` | Pass | The user chooses a product destination, not a protocol or connector layer. |
| Send warning saying `This conversation is not ready for protected send. Verify the person before sending protected content.` | Pass | The UI gives a plain consequence and a next safe action. |
| Status label saying `Result unknown` after an unverified cleanup attempt | Pass | Uncertainty remains explicit instead of being promoted to success. |
| Onboarding choice named `Select a provider adapter` | Fail | The user must understand implementation machinery before acting. |
| Warning label saying `Ratchet receipt missing; inspect browser profile` | Fail | The UI exposes transport, receipt and profile machinery instead of a product consequence. |

The negative fixtures must be refused even if the underlying implementation is
complete. The positive fixtures must remain acceptable even if the connector,
storage or protocol implementation changes.

## Machine-readable product-contract acceptance fixtures

```json
{
  "schema": "osl-subjective-design-feel-contract-v1",
  "unit": "j1",
  "contract": "complexity-hiding-product-model",
  "allowed_product_nouns": [
    "protection state",
    "trusted people",
    "connected accounts",
    "private conversations",
    "cleanup actions",
    "activity history"
  ],
  "banned_user_facing_concepts": [
    "keyservers",
    "ratchets",
    "receipts",
    "browser profiles",
    "provider adapters",
    "protocol state",
    "storage layout",
    "automation internals",
    "transport plumbing",
    "service-adapter mechanics"
  ],
  "cases": [
    {
      "name": "product navigation choices",
      "surface": "navigation",
      "visible_choices": [
        "protection state",
        "trusted people",
        "connected accounts",
        "private conversations",
        "cleanup actions",
        "activity history"
      ],
      "main_screen_answer": "Protection is ready for this private conversation.",
      "expected": "pass"
    },
    {
      "name": "implementation navigation choices",
      "surface": "navigation",
      "visible_choices": [
        "keyservers",
        "ratchets",
        "browser profiles",
        "provider adapters"
      ],
      "main_screen_answer": "Select the ratchet state before sending.",
      "expected": "fail"
    },
    {
      "name": "plain refusal with safe action",
      "surface": "warning",
      "main_screen_answer": "Protected send is not ready for this conversation.",
      "refusal": {
        "consequence": "Protected send is not ready for this conversation.",
        "safe_action": "Verify the person or send normally."
      },
      "expected": "pass"
    },
    {
      "name": "mechanism refusal",
      "surface": "warning",
      "main_screen_answer": "Ratchet receipt is missing for this provider adapter.",
      "refusal": {
        "consequence": "Ratchet receipt is missing for this provider adapter.",
        "safe_action": "Open protocol diagnostics."
      },
      "expected": "fail"
    },
    {
      "name": "secondary support export",
      "surface": "advanced-support-export",
      "main_screen_answer": "The result is unknown. Review the latest activity or try again.",
      "support_export": {
        "machine_fields_secondary": true,
        "fields": [
          "protocol state",
          "storage layout",
          "transport plumbing"
        ]
      },
      "expected": "pass"
    },
    {
      "name": "support fields replace main answer",
      "surface": "main-screen-status",
      "main_screen_answer": "Protocol state blocked by storage layout.",
      "support_export": {
        "machine_fields_secondary": false,
        "fields": [
          "protocol state",
          "storage layout"
        ]
      },
      "expected": "fail"
    }
  ]
}
```
