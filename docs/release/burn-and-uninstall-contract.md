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
    "deletes_osl_data": true,
    "offers_one_identity_backup": true,
    "backup_filename": "OSL identity backup.json",
    "offers_one_local_data_backup": true,
    "backup_directory": "OSL local data backup",
    "is_a_burn": false,
    "requires_separate_user_action": true
  },
  "claims": {
    "remote_data_unrecoverable_only_after": "server confirmation",
    "burn_status_before_server_confirmation": "pending",
    "uninstall_status": "application removed; local OSL data removed; identity backup kept only if selected"
    "uninstall_status": "application removed; local OSL data removed; local data backup kept only if selected"
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

The current NSIS uninstaller must be described honestly: uninstalling the application removes local
OSL data after offering to write exactly one identity backup file first. Keeping the backup leaves
that single file outside OSL's application data roots and removes the rest of OSL's local data.
Declining the backup removes the backup file too. This is still not a burn: it does not remove
server relay blobs, peer copies, provider messages, browser cookies, native-app history,
OSL data after offering to write exactly one local-data backup directory first. Keeping the backup
leaves that single directory outside OSL's application data roots and removes the rest of OSL's
local data. Declining the backup removes any prior backup too. This is still not a burn: it does not
remove server relay blobs, peer copies, provider messages, browser cookies, native-app history,
screenshots, exports, or any backups outside OSL's control.

The offered Documents backup is one same-Windows-user copy of identity and local
message data, not independent recovery. Its provider, region, account, control
plane, administrator, credential and key authority are those of Documents;
observed independent isolation and guaranteed recovery are both 0. A shared
failure can lose both the installed data and the copy. Hosted task 6582 is held
under owner ruling T7 because genuine disaster isolation is wanted but not
funded or operated for this release. That hold does not change the rule above:
declining the backup removes the prior backup, and other deletion/erasure duties
remain active.

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

## Temporary uninstall inventory

The uninstaller may remove OSL-owned temporary diagnostics and staging artifacts that are outside
the user data roots above. This is not a Burn claim: message stores, identities, settings, provider
profiles, and downloaded user files stay governed by the uninstall footprint map. Each section
below has exactly one `marked_for_uninstall` item so the direct checker can prove the uninstaller's
temporary-data removal work covers temporary files, logs, and crash dumps without silently omitting
one class.

```json temporary-uninstall-inventory
{
  "schema_version": 1,
  "sections": [
    {
      "name": "temporary files",
      "items": [
        {
          "name": "device transfer scratch directories",
          "marked_for_uninstall": true,
          "uninstall_action": "remove_on_uninstall",
          "locations": [
            "%LOCALAPPDATA%\\Temp\\osl-transfer-*"
          ],
          "removal_work": "remove matching OSL transfer scratch directories during uninstall cleanup",
          "source": "apps/osl-hub/src/device_transfer.rs creates std::env::temp_dir()/osl-transfer-<suffix> scratch directories and best-effort removes them after use"
        }
      ]
    },
    {
      "name": "logs",
      "items": [
        {
          "name": "temporary diagnostic and startup logs",
          "marked_for_uninstall": true,
          "uninstall_action": "remove_on_uninstall",
          "locations": [
            "%LOCALAPPDATA%\\Temp\\osl-diagnostics.log",
            "%LOCALAPPDATA%\\Temp\\osl-diagnostics.log.1",
            "%LOCALAPPDATA%\\Temp\\osl-startup-trace.txt"
          ],
          "removal_work": "delete OSL-owned temporary diagnostic and startup trace files during uninstall cleanup",
          "source": "apps/osl-hub/src/diagnostics.rs writes osl-diagnostics.log in std::env::temp_dir(); apps/osl-hub/src/main.rs and native_window_host.rs write osl-startup-trace.txt"
        }
      ]
    },
    {
      "name": "crash dumps",
      "items": [
        {
          "name": "Windows Error Reporting dumps for OSL executables",
          "marked_for_uninstall": true,
          "uninstall_action": "remove_on_uninstall",
          "locations": [
            "%LOCALAPPDATA%\\CrashDumps\\OSL Privacy*.dmp",
            "%LOCALAPPDATA%\\CrashDumps\\osl-hub*.dmp",
            "%LOCALAPPDATA%\\Microsoft\\Windows\\WER\\ReportArchive\\AppCrash_OSL*",
            "%LOCALAPPDATA%\\Microsoft\\Windows\\WER\\ReportQueue\\AppCrash_OSL*"
          ],
          "removal_work": "remove OSL-named Windows crash dump and WER report artifacts during uninstall cleanup",
          "source": "Windows may persist crash dumps for the OSL Privacy / osl-hub process outside OSL data roots; the uninstall inventory treats only OSL-named dump artifacts as removable"
        }
      ]
    }
  ]
}
```

## Windows uninstall inventory extensions

The uninstall inventory also covers Windows-owned integration surfaces outside the seven
user-actionable places in the footprint map. These rules are ownership-bounded: an uninstall may
remove only records attributable to OSL, and must not remove provider browser data, another
application's registry values, jobs, startup entries, or services. Each record is one discovery and
removal rule, so its section's `count` must equal the number of records in that section.

```json windows-uninstall-inventory
{
  "schema_version": 1,
  "sections": [
    {
      "name": "OSL-owned browser records",
      "count": 1,
      "records": [
        {
          "name": "OSL app-owned browser profile data",
          "locations": [
            "%LOCALAPPDATA%\\org.oslprivacy.hub\\EBWebView\\",
            "%LOCALAPPDATA%\\org.oslprivacy.hub\\service-profiles-v2\\",
            "%LOCALAPPDATA%\\org.oslprivacy.hub\\browser-companion-profiles-v1\\"
          ],
          "ownership": "the record is below an org.oslprivacy.hub app-owned WebView2 or isolated companion-browser profile root; provider-owned browser profiles are excluded",
          "uninstall_action": "inventory and remove the OSL-owned profile records during uninstall cleanup",
          "source": "src-tauri/src/main.rs documents the org.oslprivacy.hub WebView2 profile; apps/osl-hub/src/cleanup.rs names service_profiles and browser_companion_profiles as OSL-owned purge targets"
        }
      ]
    },
    {
      "name": "Windows registry keys",
      "count": 1,
      "records": [
        {
          "name": "OSL application registration records",
          "locations": [
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\<OSL-owned key>",
            "HKLM\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\<OSL-owned key>",
            "HKLM\\Software\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\<OSL-owned key>"
          ],
          "ownership": "the key identifies OSL Privacy by exact DisplayName or an OSL-owned key name and its uninstall command resolves to the OSL install root",
          "uninstall_action": "inventory and remove only the matching OSL registration keys",
          "source": "scripts/qa/measure-uninstall-residue.ps1 inventories the three Windows uninstall hives and bounds matches to OSL DisplayName or key names"
        }
      ]
    },
    {
      "name": "scheduled jobs",
      "count": 1,
      "records": [
        {
          "name": "OSL Task Scheduler jobs",
          "locations": [
            "Task Scheduler Library\\OSL Privacy\\*"
          ],
          "ownership": "the task is inside the OSL Privacy task folder and every executable action resolves to the OSL install root",
          "uninstall_action": "inventory, stop, and unregister each matching OSL job",
          "source": "the Windows uninstall contract reserves a bounded OSL Privacy Task Scheduler folder for OSL-owned scheduled work; task 3706 exercises this inventory rule"
        }
      ]
    },
    {
      "name": "startup entries",
      "count": 1,
      "records": [
        {
          "name": "OSL Privacy logon startup entry",
          "locations": [
            "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\OSL Privacy"
          ],
          "ownership": "the registry value name is exactly OSL Privacy and its command resolves to the installed OSL Privacy executable",
          "uninstall_action": "inventory and delete the exact OSL Privacy Run value",
          "source": "src-tauri/src/windows_startup.rs defines RUN_KEY and OSL_STARTUP_VALUE and changes only that exact value"
        }
      ]
    },
    {
      "name": "background services",
      "count": 1,
      "records": [
        {
          "name": "OSL Privacy Windows services",
          "locations": [
            "Windows Service Control Manager entries with an OSL Privacy display name and an executable under the OSL install root"
          ],
          "ownership": "both the service display name begins with OSL Privacy and the registered executable resolves inside the OSL install root",
          "uninstall_action": "inventory, stop, and delete each matching OSL service",
          "source": "the Windows uninstall contract requires dual name-and-binary ownership before a background service is attributed to OSL; task 3706 exercises this inventory rule"
        }
      ]
    }
  ]
}
```

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
- Describe uninstall as “Remove the OSL app from Windows.” Pair it with “This removes local OSL
  data after offering one identity backup” unless a future uninstaller changes that behavior and
  this contract is revised with a test.
  data after offering one local backup” unless a future uninstaller changes that behavior and this
  contract is revised with a test.
- Never equate uninstall with burn, remote deletion, provider-message deletion, or peer-copy
  deletion. Never equate an account-level burn with removal of the Windows app.

## Implementation boundary

This contract defines the product truth, including the required offline queue and pending state.
It does not claim that every remote-delete or peer-delete mechanism is implemented today. Until a
mechanism can persist its request and report the required acknowledgement, the relevant UI and
site copy must not claim that effect.
