# Device roster is a hard prerequisite

Status: dependency contract for T6 storage and T21 Spaces. This is an analysis
contract, not an endpoint implementation.

## Verified current state

The keyserver currently has 40 migrations. None contains a device identifier or
device-roster row. An account is therefore the only recipient target the
keyserver can represent today.

That is incompatible with OWNER DECISION D17: delivery is one independent copy
per device, and each copy is deleted only when *that device* acknowledges it.
Without a roster, a sender has exactly one representable target per account.
Calling that multi-device support would silently omit a recipient's second
device; it is not a degraded delivery mode that may be enabled temporarily.

## DEP-1: device roster

Before any D17 fan-out is enabled, the identity track must define and T6 must
implement an account-scoped roster with these properties:

1. Each active device has an opaque, stable `device_id` and a distinct
   device prekey bundle. An account identity key is not a device bundle.
2. Registration and revocation are authenticated account operations. A revoked
   or unregistered device is absent from subsequent target sets.
3. The target set for an account is all currently active device bundles, not
   one account-level fallback target. A sender encrypts and uploads a separate,
   unrelated D17 copy for every target.
4. The roster is used to obtain encryption targets; it must not make the blob
   store correlate those per-device copies. Fetch remains capability-authorized
   and each copy has its own acknowledgement lifecycle.
5. Device Transfer (D34) is an explicit, owner-authenticated provisioning path:
   the destination proves possession before it is registered, works without a
   server round trip, and deliberately retains or destroys the old device's
   copy. It must not weaken anchor-clone detection.

T6-K6 owns the initial roster endpoint and migration. The identity track owns
pairing/linking semantics. Spaces and any other fan-out feature consume the
roster; they must not invent a parallel device list.

## Gate

No feature may claim D17 multi-device delivery, enable Space fan-out, or treat
a second device as a delivery target until DEP-1 has an executable roster
contract and its two-device test passes. A single-device implementation is
valid only when it is labelled and enforced as single-device delivery.

## Contract test vectors

```json
{
  "version": 1,
  "deviceRosterRequiredBefore": ["d17-multi-device-delivery", "space-fan-out"],
  "singleDevice": {
    "activeDeviceIds": ["device-a"],
    "targetDeviceIds": ["device-a"],
    "expect": "deliver"
  },
  "twoDevices": {
    "activeDeviceIds": ["device-a", "device-b"],
    "targetDeviceIds": ["device-a", "device-b"],
    "expect": "deliver"
  },
  "staleDevice": {
    "activeDeviceIds": ["device-a"],
    "targetDeviceIds": ["device-a", "device-b"],
    "expect": "reject"
  },
  "requirements": {
    "distinctPrekeyBundlePerDevice": true,
    "perDeviceCopy": true,
    "perDeviceAcknowledgement": true,
    "noAccountFallbackTarget": true,
    "deviceTransferExplicitAndOwnerAuthenticated": true,
    "deviceTransferDestinationProvesPossession": true,
    "deviceTransferWorksOffline": true
  }
}
```
