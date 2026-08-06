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

## Uninstall footprint map

This is the full uninstall-facing map of places OSL writes to in the current Hub build. It groups
the footprint by user-actionable location. The `count` field is deliberately one per named place:
the direct checker reports these seven names and only these seven names unless a new place is added
to this machine-readable list.

```json uninstall-footprint-map
{
  "schema_version": 1,
  "places": [
    {
      "name": "program files",
      "count": 1,
      "locations": [
        "%ProgramFiles%\\OSL Privacy\\ or the per-user NSIS install root for the OSL Privacy executable, bundled WebView assets, icons and updater metadata"
      ],
      "source": "apps/osl-hub/tauri.conf.json declares productName OSL Privacy, identifier org.oslprivacy.hub and the NSIS bundle target"
    },
    {
      "name": "settings",
      "count": 1,
      "locations": [
        "%APPDATA%\\org.oslprivacy.hub\\preview-preferences.json",
        "%APPDATA%\\org.oslprivacy.hub\\burn-review-state.json",
        "%APPDATA%\\org.oslprivacy.hub\\tor-preference.json",
        "%APPDATA%\\org.oslprivacy.hub\\service-registry.json",
        "%APPDATA%\\org.oslprivacy.hub\\service-scope-index.json",
        "%APPDATA%\\org.oslprivacy.hub\\browser-footprint.json",
        "%APPDATA%\\org.oslprivacy.hub\\deadman-bindings.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\app_preferences.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\allowed_places.sqlite"
      ],
      "source": "apps/osl-hub/src/main.rs startup loads these config files; apps/osl-hub/src/cleanup.rs lists the same roots; crates/ipc/src/allowed_places.rs writes allowed_places.sqlite"
    },
    {
      "name": "keys",
      "count": 1,
      "locations": [
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\identity.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\password_marker.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\pending_rotation.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\keyserver.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\accounts\\<slot>\\identity.json",
        "Windows Credential Manager / OS keyring entry used by KeyringSealer",
        "Windows TPM or NCrypt persisted key when that sealer is available"
      ],
      "source": "crates/keystore/src/recipients.rs resolves the OSL base/config directory; crates/ipc/src/state.rs and crates/keystore/src/duress.rs bind identity.json and the keyring/TPM material"
    },
    {
      "name": "stored messages",
      "count": 1,
      "locations": [
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\store\\messages.sqlite and WAL/SHM siblings",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\rn-sessions\\",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\message_open_clock.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\receipt_dedup.json",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\revocation_outbox.json",
        "WebView2 localStorage under the org.oslprivacy.hub profile for OSL Chat local state"
      ],
      "source": "crates/ipc/src/fresh_start.rs names store/messages.sqlite; crates/ipc/src/wire_rn.rs stores RN sessions; apps/osl-hub/src/message_expiry.rs stores message clocks; apps/osl-hub/src/osl_chat_local_state_key.rs documents renderer localStorage state"
    },
    {
      "name": "downloaded files",
      "count": 1,
      "locations": [
        "%APPDATA%\\org.oslprivacy.hub\\components-v1\\component-payloads\\",
        "%APPDATA%\\org.oslprivacy.hub\\osl-core\\osl-assets\\chunks\\",
        "%LOCALAPPDATA%\\org.oslprivacy.hub\\peer-attachment-staging\\"
      ],
      "source": "apps/osl-hub/src/cleanup.rs lists components-v1; apps/osl-hub/src/osl_assets.rs stores asset chunks under osl_config_dir; apps/osl-hub/src/message_expiry.rs and peer_attachment_io.rs use peer-attachment-staging"
    },
    {
      "name": "startup entry",
      "count": 1,
      "locations": [
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\OSL Privacy or a Startup-folder shortcut created for an OSL-owned autostart install"
      ],
      "source": "the uninstall residue script must inventory Windows uninstall/startup registry surfaces; no product code may treat this as data deletion"
    },
    {
      "name": "logs",
      "count": 1,
      "locations": [
        "%LOCALAPPDATA%\\Temp\\osl-diagnostics.log",
        "%LOCALAPPDATA%\\Temp\\osl-diagnostics.log.1",
        "%LOCALAPPDATA%\\Temp\\osl-startup-trace.txt",
        ".profiles\\<qa-profile>\\logs\\backend.log when a harness redirects stderr"
      ],
      "source": "apps/osl-hub/src/diagnostics.rs writes capped diagnostic logs; apps/osl-hub/src/main.rs writes the startup breadcrumb trace"
    }
  ]
}
```

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
