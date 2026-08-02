# Lifecycle contract

**Status:** frozen by T15-F1. This section defines the USB dead-man switch.
It is a trigger for existing session-lock and burn cleanup mechanisms, not a
new erase mechanism.

## USB dead-man switch

```deadman-contract
{
  "binding": {
    "scope": "one configured removable volume per setting",
    "identity": "runtime-provided stable volume identifier",
    "drive_letter_is_identity": false
  },
  "trigger": {
    "event": "bound_volume_removal_completion",
    "only_the_bound_volume": true,
    "arrival_does_not_trigger": true,
    "removal_while_suspended_or_hibernated_is_detectable": false
  },
  "actions": {
    "default": "lock",
    "lock": {
      "recoverable": true,
      "operation": "existing_session_lock"
    },
    "wipe": {
      "recoverable": false,
      "operation": "existing_burn_cleanup",
      "enablement": "explicit_per_device_choice_with_exact_typed_confirmation"
    }
  },
  "locked_removal": {
    "action": "remain_locked",
    "escalates_to_wipe": false,
    "reinsertion_unlocks": false
  },
  "limits": {
    "wipes_only_osl_controlled_data": true,
    "cannot_guarantee_erasure_of": [
      "Windows_swapped_data",
      "Windows_hibernated_data",
      "Windows_cached_data"
    ],
    "physical_seizure_protection": "strong_not_forensic_guarantee"
  }
}
```

A binding belongs to the selected device setting, never to every removable
device. The runtime must compare the stable volume identifier supplied by the
Windows device-notification path; a drive letter is not a device identity.

Removal means Windows reports completion of removal for the bound volume. A
device arrival, removal of an unbound device, or a drive-letter reassignment
does not trigger an action. The monitor applies the chosen action once to the
bound volume's removal completion; repeated notifications must not turn a
lock choice into a wipe.

**Lock** is the default and is recoverable: it invokes the existing session
lock and requires the normal local unlock path afterwards. **Wipe** invokes
the existing burn cleanup target set. It is destructive and may be enabled
only after an explicit per-device choice and an exact typed confirmation; it
is never the fallback for an incomplete confirmation.

If the bound stick is removed while the account is already locked, it remains
locked. That event does not escalate to wipe, even when another device's
setting is wipe, and putting the stick back never unlocks the account.

OSL can erase only data OSL controls. Windows can already have swapped,
hibernated, or cached copies, so this is strong protection against physical
seizure, not a guarantee against forensic recovery. Windows does not report
removal while the machine is suspended or hibernated; the switch makes no
claim that an absence occurring during either state was detected.
