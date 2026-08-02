# Enclaves role contract

**Status:** frozen by T21-F1. This contract defines the v1 Enclaves role
model. It uses the owner’s final product name, **Enclaves** (D84).

## Boundary

Roles grant governance capability, never visibility. An Enclave role does not
contain a channel identifier, a channel key, a recovery key, or an owner key.
There is no role that reads a channel without being a member of that channel.
Channel membership and possession of the channel's current keys are the sole
basis for reading its future content; they are modeled separately from a role.

An administrator who needs to read a channel joins it under the same rules as
every other member. No role grants historical content: admitting or promoting
a member provides future keys only after rotation. The relay receives neither
the roster nor channel keys.

Role actions are governance requests. Their enforcement classification and
channel-membership/key checks are intentionally owned by T21-F2 and T21-F3,
respectively; this contract does not label a client-side role check as
cryptographically enforced.

## Fixed v1 roles

The complete role set is `member`, `moderator`, and `admin`. Custom roles are
not supported in v1. The permitted governance requests are deliberately small:

| Role | Governance requests | Does the role grant channel visibility? |
| --- | --- | --- |
| `member` | none | No |
| `moderator` | moderate members | No |
| `admin` | moderate members; manage roles; manage channels | No |

The machine-readable contract is the test surface for T21-T31:

```space-role-contract
{
  "roles": [
    { "name": "member", "governance_capabilities": [] },
    { "name": "moderator", "governance_capabilities": ["moderate_members"] },
    { "name": "admin", "governance_capabilities": ["moderate_members", "manage_roles", "manage_channels"] }
  ],
  "custom_roles": false,
  "visibility": {
    "basis": "channel_membership_and_current_key_possession",
    "role_grants_channel_visibility": false,
    "role_grants_history": false,
    "owner_or_recovery_key_exists": false
  }
}
```
