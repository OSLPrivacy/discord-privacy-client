# OSL update prompt policy

This is the policy T7 uses when presenting updater state. It preserves the
current default: OSL can discover an update, but it never downloads, installs,
or restarts because an update check found one.

```json
{
  "check": "after_unlock_or_workspace_start_and_manual",
  "availability": "passive_persistent_banner",
  "security_update": "strongly_recommended_not_forced",
  "install": "explicit_confirmed_click_only",
  "offline": "no_retry_loop_no_false_up_to_date_claim",
  "network_signal": "check_contacts_update_endpoint"
}
```

## Ordinary updates

After a successful non-onboarding unlock, and when an already-unlocked workspace
starts, OSL performs one background check. The user can also choose **Check for
updates** from Settings. If a signed update is
available, OSL shows a passive, persistent availability banner and an Updates
page action. It does not open a modal merely because the check succeeded.

The banner remains available across app restarts until the installed version
changes. Choosing **Not now** closes only the current dialog; it is not consent
to install and does not suppress the later availability notice. Release notes
are plain text and the app never renders remote release-note HTML.

## Security updates

A release may be marked security-critical only through the reviewed release
process. T7 must label that state plainly as a security update and make its
recommended action visible in the same persistent prompt. It may use stronger
language and keep the notice prominent, but it must not force a download,
installation, restart, or block ordinary use. A security fix that a user never
accepts remains unapplied; OSL must state that honestly rather than implying it
has protected the device already.

## Installation and restart

Installation begins only after the user selects **Install**, sees the version,
release notes, restart/unsaved-work warning, and then explicitly selects
**Install & restart**. The updater rechecks the offered version and verifies
the update package against OSL's updater key before installing it. A check,
banner, notification, or security classification is never installation consent.

## Offline and privacy behavior

Offline or failed checks remain non-fatal: OSL reports that the update status
is unavailable or failed, does not claim the app is up to date, and does not
spin in a retry loop. The next scheduled opportunity is the next successful
unlock or workspace start, or a manual check by the user.

Each check contacts OSL's configured update endpoint. That request can reveal
to the endpoint and the network that this device is checking for an OSL update;
it is therefore limited to the launch check and an explicit manual action. The
app sends no UI telemetry as part of the prompt.

## Implementation boundary

The current client already supplies the one background launch check, passive
availability banner, manual check, and explicit install-confirmation flow. T7
owns prompt presentation, including the reviewed security-update label and any
accessibility treatment; this document does not authorize a different updater
transport or an automatic-install path.
