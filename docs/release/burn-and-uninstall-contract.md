# Burn and uninstall contract

This is the single source of truth for T7's in-app strings and T11's download-site copy. It
distinguishes removing OSL data from removing the OSL application. Neither action removes data
held by other applications, operating systems, services, recipients, screenshots, exports, or
backups outside OSL's control.

```json burn-uninstall-contract
{
  "burn": {
    "local_effect": "destroy OSL's local copy immediately",
    "local_immediate": true,
    "uninstalls_app": false,
    "remote_delete": "queue while offline; request deletion while online",
    "remote_completion": "confirmed only after the server acknowledges deletion",
    "peer_delete": "queue while offline; request deletion while online",
    "peer_completion": "confirmed only after the peer acknowledges deletion"
  },
  "uninstall": {
    "removes_application": true,
    "deletes_osl_data": false,
    "is_a_burn": false,
    "requires_separate_user_action": true
  },
  "claims": {
    "remote_data_unrecoverable_only_after": "server confirmation",
    "burn_status_before_server_confirmation": "pending",
    "uninstall_status": "application removed; data deletion is not implied"
  }
}
```

## What “Burn” means

Burn immediately destroys the local OSL copy. It is available offline for that local effect.
Burn does **not** uninstall OSL or remove the application from Windows.

The remote effects are separate and must be shown separately:

| Effect | Online | Offline | Completion claim |
|---|---|---|---|
| Local OSL copy | Destroy now | Destroy now | Complete after the local operation finishes |
| Server relay blob | Send deletion request | Queue request for reconnect | Complete only after server confirmation |
| Peer copy | Send deletion request | Queue request for reconnect | Complete only after peer confirmation |

Until the server confirms deletion, copy must describe the burn as **pending** for the server
effect. It must not say that the message “can never be retrieved again,” that the server copy was
destroyed, or that a peer copy was deleted. A failed or unsent request remains pending; it is not a
successful remote burn.

Burn is limited to what OSL controls. It does not remove provider messages, login profiles,
browser cookies, provider history, native-app history, screenshots, exports, backups, or copies a
recipient has already made unless a separately confirmed mechanism says otherwise.

## What “Uninstall” means

Uninstall removes the OSL application through Windows. It is a separate, optional user action that
may be offered after an account-level burn, but never presented as a burn effect.

The current NSIS uninstaller must be described honestly: uninstalling the application does not by
itself delete OSL data. Users who want data removal must use Burn (and wait for its remote
confirmation where that matters); users who want to remove the executable must then uninstall
separately. Conversely, users may uninstall without burning, and the copy must not imply that this
deletes their OSL data.

## Required wording rules for T7 and T11

- “Burn local data” is appropriate for the local action. “Uninstall OSL” is a separate action.
- When offline or awaiting acknowledgement, say “Local copy removed. Server deletion pending” (and
  name peer deletion as pending when applicable).
- Only after acknowledgement may copy say “Server deletion confirmed” or “Peer deletion
  confirmed.” Do not collapse either into the local completion state.
- Describe uninstall as “Remove the OSL app from Windows.” Pair it with “This does not delete OSL
  data” unless a future uninstaller changes that behavior and this contract is revised with a test.
- Never equate uninstall with deletion, or an account-level burn with removal of the Windows app.

## Implementation boundary

This contract defines the product truth, including the required offline queue and pending state.
It does not claim that every remote-delete or peer-delete mechanism is implemented today. Until a
mechanism can persist its request and report the required acknowledgement, the relevant UI and
site copy must not claim that effect.
