# Browser-import coverage (TI-4)

Browser import is separately consented from deletion. A grant identifies one
browser and one profile; it is consumed before that profile directory is
resolved. The receipt records the selected browser/profile and every profile
actually read, including an empty read.

| Browser | Selected-profile mechanism | QA result | Receipt / revocation check |
| --- | --- | --- | --- |
| Chrome | `History` snapshot after a Chrome-only grant | Pending Windows VM evidence | Receipt names Chrome/selected profile; revoke that profile only |
| Edge | `History` snapshot after an Edge-only grant | Pending Windows VM evidence | Receipt names Edge/selected profile; revoke that profile only |
| Firefox | `places.sqlite`; username decryption needs a separate opt-in and returns username-only data | Pending Windows VM evidence | Receipt names Firefox/selected profile; no password-material path |
| Brave | `History` snapshot after a Brave-only grant | Pending Windows VM evidence | Receipt names Brave/selected profile; revoke that profile only |
| Opera | `History` snapshot after an Opera-only grant | Pending Windows VM evidence | Receipt names Opera/selected profile; revoke that profile only |

## TI-4 execution

For each row, create two browser profiles and grant only the first. Verify the
second profile and every other browser are unread. Attempt to substitute a
deletion consent for the import grant and require refusal. Revoke the first
profile and verify only its imported footprint is removed. Store the VM
receipt, profile list, and mechanism outcome with the test run; “pending” is
not a release pass.
