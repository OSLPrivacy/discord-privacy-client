# Lost-all-devices recovery v1

This protocol is for an account whose devices are all unavailable. It is not
ordinary device pairing: pairing still requires confirmation on an existing
authorized device.

## Recovery kit

Schema name: `osl-lost-all-devices-recovery-kit/v1`

The binary kit is the following fixed-width concatenation:

```
"OSL-lost-all-devices-recovery-kit-v1"
|| recovery_epoch_be_u64
|| authority_public_ed25519[32]
|| authority_secret_ed25519_seed[32]
```

The kit is private. The service stores only `authority_public_ed25519` and
`recovery_epoch`.

## Declaration

Schema name: `osl-lost-all-devices-recovery-declaration/v1`

The canonical bytes signed with the current kit authority are:

```
"OSL-lost-all-devices-recovery-declaration-v1"
|| recovery_epoch_be_u64
|| replacement_device_ed25519_public[32]
|| successor_recovery_authority_ed25519_public[32]
```

The detached Ed25519 signature is 64 bytes. The epoch must be exactly one more
than the stored current epoch. The successor authority must be nonzero and
different from the current authority. Changing the domain, epoch, replacement
key, or successor authority invalidates the signature.

An independent verifier needs only the current kit public authority and the
published fields above. It does not contact the service.

## Atomic transition

The service verifies the signature against its current authority and performs
one serialized compare-and-consume transition:

1. replace the device roster with exactly the declared replacement key;
2. replace the current recovery authority with the declared successor;
3. advance the recovery epoch by exactly one; and
4. publish the winning signed declaration.

Only one of two claims against the same authority and epoch can commit. After
the commit, the predecessor signature no longer verifies against the current
authority. There is no recovery-success counter, terminal epoch, or maximum
number of recoveries. Every accepted recovery must have a distinct saved
successor kit, so the same transition can repeat indefinitely.
