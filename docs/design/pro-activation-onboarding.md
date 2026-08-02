# Pro activation during onboarding

**Owner:** T16 specifies this contract. T15 places and renders the step. T7
renders entitlement status elsewhere. This document is the handoff for the
shared `pro` onboarding route.

## Purpose and placement

This is an optional step between recovery and Privacy. It lets a person apply
an activation code they already have; it is not a purchase, account-creation,
or cloud-AI consent screen. Pro unlocks optional AI features, but never basic
encryption or the word-bank carrier. Cloud generation needs its own later,
explicit consent.

The normal continuation for every outcome is **Privacy**. Do not jump directly
to Sending: the person must still make the Tor and delivery-presence choices.

## Initial state and controls

Show a small ticket/key icon with a reduced-motion-safe explanatory animation.
The static first frame must make sense by itself. Use plain language:

> **Optional**
>
> ## Enter Pro code
>
> Have a Pro code? Enter it now. You can also add it later in Settings. Free
> OSL stays usable without one.

Controls, in this order:

1. An accessible input labelled **Pro activation code**, with the example
   `OSL-XXXX-XXXX-XXXX-XXXX`. It accepts the normalized code format only.
2. Primary button: **Continue**.
3. Visible secondary button: **Skip**.

Skip is first-class, not a hidden link, a disabled state, or a confirmation
dialog. Selecting it makes no activation request, saves no code or
activation-attempt state, and continues to Privacy with Free access. It must
remain available after a failed attempt. The person can paste a code later in
Settings.

## Submission states and copy

Never echo the code after submission, put it in a URL, or persist it in web
storage. While a valid code is being checked, clear the input, disable the
primary button, and label it **Activating…**; Skip remains available only
until the request begins, so one form submission cannot issue two requests.
The native activation seam owns redemption/validation semantics and returns a
`HubLicenseState`; this screen does not infer a payment method, entitlement
duration, or a new account.

| Result | In-step state and exact copy | Available action | Continuation |
|---|---|---|---|
| Active Pro or offline grace returned | Show **Pro is ready** and **Pro features are available on this device.** | **Continue** | Privacy, retaining the returned access state |
| Code has no active Pro access | Show **This code does not include active Pro access.** | **Try again** and **Skip** | Stay on this step until chosen |
| Code not recognized | Show **We could not find that code. Check it and try again.** | **Try again** and **Skip** | Stay on this step until chosen |
| Code revoked | Show **This code is no longer available.** | **Try again** and **Skip** | Stay on this step until chosen |
| Code expired | Show **This code has expired.** | **Try again** and **Skip** | Stay on this step until chosen |
| Code pending | Show **This code is not ready yet. Try again later.** | **Try again** and **Skip** | Stay on this step until chosen |
| Network, native, or unknown failure | Show **We could not activate Pro right now. Check your connection and try again.** | **Try again** and **Skip** | Stay on this step until chosen |

An invalid or blank locally entered value does not send a request. Show
**Enter the activation code shown after checkout** and leave the code editable.
Do not describe any failure as cancellation, a subscription, billing, a lost
purchase, or loss of the person's messages.

## Existing entitlement states

If onboarding re-enters this route with active Pro or offline grace already
saved, do not ask for a code again. Show **OSL Pro is ready** and a single
**Continue** control to Privacy. If the app knows an entitlement is lapsed,
unavailable, or cannot be checked, use the T16-F1 view and T16-F2 copy. In
particular, a lapse must say that Free OSL keeps working; activation never
threatens existing encrypted messages, sending, receiving, or the word-bank
carrier.

## Test fixture

The fixture below is intentionally machine-readable so the contract test can
catch an accidental loss of the Skip guarantee or an unsafe route change.

```json pro-activation-contract
{
  "placement": { "before": "privacy", "after": "recovery" },
  "skip": { "firstClass": true, "networkRequest": false, "persistAttempt": false, "route": "privacy", "access": "free" },
  "success": { "statuses": ["ACTIVE", "CANCELLED", "GRACE"], "route": "privacy" },
  "failure": { "staysOnStep": true, "canRetry": true, "canSkip": true },
  "safety": { "basicEncryptionRequiresPro": false, "cloudConsentBundledWithActivation": false, "codePersistedInWebStorage": false }
}
```
