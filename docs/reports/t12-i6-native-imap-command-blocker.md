# T12-I6 native IMAP command blocker

The independent verification surface (T12-I5) and typed confirmation gate
(T12-R6) are present. The required native mutation adapter is not: this
checkout contains only `SeededLocalImapFixture` in `attended_imap.rs`, and
`scrub_imap.rs` deliberately returns `NativeDeletionDisabled` without a
`NativeImapAdapter` implementation.

Registering `scrub_imap_prepare_delete` or `scrub_imap_delete` against that
fixture would expose a destructive-looking UI command which cannot act on an
account. Registering a command that routes to the existing disabled wrapper
would make the UI's current invocation reachable only to fail at runtime. Both
would violate the T12-I6 end-to-end and honest-reachability requirements.

No Tauri command, permission, or capability grant is added until a reviewed
native IMAP adapter exists. The exact missing implementation is the T12-I4
native adapter boundary, not the verification or confirmation prerequisites.
