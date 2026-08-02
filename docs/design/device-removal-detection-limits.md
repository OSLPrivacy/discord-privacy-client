# Device-removal detection limits

Status: **not measured on a Windows VM** (2026-08-02).

This document is the source of truth for the dead-man-switch UI. It records
only what the implementation and its tests establish; it does not turn an
unmeasured timing or platform behaviour into a product claim.

## Machine-checked facts

| Fact | Evidence | UI consequence |
| --- | --- | --- |
| Detection is Windows-only. | `crates/runtime/src/usb.rs` registers Win32 device notifications; non-Windows `UsbMonitor` is a no-op. | Do not offer or describe this removal trigger as protecting Linux or macOS. |
| A safe eject and a surprise yank are both routed when Windows delivers `DBT_DEVICEREMOVECOMPLETE`. | `crates/runtime/tests/usb_removal_test.rs` exercises both message fixtures and asserts the removal callback receives the opaque volume interface path. | The UI may say that a running Windows session reacts to a delivered removal notification; it must not promise a deadline. |
| A drive letter is not the device identity. | The same test rejects `F:` and retains only a `\\?\\Volume{...}` interface path. | Never display a drive letter as the protected-device key or imply that it remains stable after remount. |
| Removal is not detectable while the VM is suspended or hibernated. | The application has no executing thread while suspended; no Windows-resume measurement has been recorded. | State this limit directly. Do not imply continuous protection through sleep or hibernation. |
| Surprise-yank delivery latency is unmeasured. | TD-1 proves message routing, not end-to-end VM timing. No TD-2 result artifact exists. | Do not show a latency number, an “instant” claim, or a timeout derived from an assumed notification delay. |

## Required TD-2 measurement before a timing claim

Run this on the release Windows VM with the actual removable-volume fixture,
not a synthetic `WM_DEVICECHANGE` call:

1. Record a monotonic timestamp immediately before a surprise yank.
2. Record the timestamp at the dead-man callback and retain both raw values.
3. Suspend or hibernate the VM, remove the same device, then resume and record
   whether the callback runs, and if so when relative to resume.
4. Save the VM image/build identifier, Windows version, device interface path
   class (not its label), iteration count, and all samples.

TD-2 is complete only when this document is updated with those raw artifacts
and derived summary statistics. Until then, the two final rows above are
deliberate product limits, not missing copy.
