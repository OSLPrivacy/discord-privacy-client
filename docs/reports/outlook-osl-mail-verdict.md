# Outlook OSL Mail Verdict

Anchor: OutlookOslMailSupportVerdict

Date recorded: 2026-07-30

## Verdict

Outlook inline-reply support as OSL Mail is `unsupported` for the current release gate.

This is not a rejection of Outlook as a future Stage A mail privacy surface. OSL may still plan an
Outlook-connected mail client for user-authorized mailbox cleanup, tracker blocking, link
sanitization, warnings, and honest labels. The unsupported claim is narrower: OSL must not present
an inline reply inside Outlook, including new Outlook, as OSL Mail protected delivery or OSL E2EE.

## Evidence

- `docs/design/osl-master-decision-2026-07-26.md` scopes Outlook as OSL Mail rather than an
  ordinary chat overlay. A service logo does not imply support unless the support matrix has the
  service-specific proof.
- `docs/design/osl-gui-final-plan.md` says OSL Mail is staged. Stage A can inspect and clean only
  mail the user explicitly authorizes, and missing authorization, mailbox binding, or provider
  support must refuse the protected action while preserving draft or warning state.
- The `m3` mail backend route accepts OSL-to-OSL delivery only after signed identity
  authorization, active sender mailbox, active recipient mailbox, and recipient consent. Absence or
  revocation of consent refuses delivery.
- Microsoft documents new Outlook on Windows as a web-add-in surface and says VSTO/COM add-ins are
  not supported there:
  https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/one-outlook
- Microsoft documents Outlook add-ins as sandboxed web components. It also says add-ins in new
  Outlook on Windows are not supported for non-Microsoft mailbox accounts:
  https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/outlook-add-ins-overview
- Microsoft event-based activation covers compose, send, and decrypt events in new Outlook, but its
  own limitations include deployment constraints, internet dependence, timeouts, current-item
  navigation termination, and non-standard compose surfaces where activation may not work:
  https://learn.microsoft.com/en-us/office/dev/add-ins/develop/event-based-activation
- Microsoft documents a custom encryption add-in path, but also documents that encrypted messages
  must be decrypted before reply/forward and that reply/forward drafts are saved unencrypted in the
  Drafts folder:
  https://learn.microsoft.com/en-us/office/dev/add-ins/outlook/encryption-decryption

## Decision

OSL must refuse the protected inline-reply mode in Outlook unless a future release has a separate
Outlook OSL Mail gate with all of the following evidence:

- A user-authorized mailbox binding for the exact account being used.
- A recipient binding that distinguishes OSL Mail recipients from ordinary external email.
- Recipient consent or another explicit recipient authority equivalent to the OSL-to-OSL backend
  rule.
- A send path that cannot silently downgrade to ordinary Outlook email when the add-in, mailbox
  policy, network, account type, or compose surface is unsupported.
- A reply/forward draft path that does not leave plaintext protected content in the Outlook Drafts
  folder or any provider-visible ordinary mailbox state.
- Runtime proof on new Outlook and the chosen classic Outlook surface, with non-Microsoft account
  behavior recorded separately instead of inferred.

Until that evidence exists, product copy and UI copy must label Outlook OSL Mail protected inline
reply as `unsupported`, `Coming soon`, or `Externally blocked`; it must not say Outlook inline
replies are OSL Mail, OSL E2EE, or protected delivery.
