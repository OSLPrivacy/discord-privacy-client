# Windows installer WebView2 network dependency

**Decision date:** 2026-07-31  
**Status:** decided

## Decision

**Decision: acceptable for the standard installer.** OSL's Windows NSIS installer will keep
Tauri's `downloadBootstrapper` mode. On a device that does not already have the Microsoft Edge
WebView2 Runtime, installation requires an internet connection during installation so Microsoft
can provide that runtime. It is not an offline installer.

This is an acceptable dependency for the small, ordinary download only when it is disclosed before
the user downloads it. It is not acceptable to describe that download as working offline, or to
silently treat a missing-runtime download failure as a successful installation.

## Current, observable behaviour

`apps/osl-hub/tauri.conf.json` configures `webviewInstallMode` as
`downloadBootstrapper` with `silent: true`. A machine that already has WebView2 does not need this
step. A machine without it makes an install-time request to Microsoft; on a disconnected or
locked-down network the install cannot finish.

The runtime is a prerequisite for the Tauri application, not an OSL optional component. The
application cannot start without it. No message content, OSL identity, or account credential is
needed for the runtime acquisition, but the network used and the fact that an install is being
attempted can be observed by the network and Microsoft.

## Why this mode

D44 requires a fully working product on a low-end device and says its baseline must be smaller,
not crippled. `offlineInstaller` would carry the WebView2 runtime in every download, adding roughly
150 MB even for the many Windows devices that already supply it. That makes the mandatory base
installer materially larger without adding an OSL feature or a removable module.

`embedBootstrapper` only embeds Microsoft's small bootstrapper; it still obtains the runtime from
the network and therefore does not create an offline install. It does not solve the disclosed
dependency or locked-down-network failure mode. `downloadBootstrapper` keeps the base download
smallest, so it is the chosen mode.

## Release and support contract

T11, which owns download-site copy, must publish all of the following beside the Windows download:

- “Internet may be required during installation if Microsoft Edge WebView2 Runtime is not already
  installed.”
- “This standard download is not an offline installer.”
- “On a managed or blocked network, ask your administrator to install/allow Microsoft Edge WebView2
  Runtime, then run OSL again.”

The installer or its launch guidance must not present installation as complete until the prerequisite
has installed and OSL can launch. Support must diagnose a failed runtime acquisition as an unmet
prerequisite, not as an OSL account, identity, or network-privacy failure.

## Revisit condition

Change this decision only with a coordinated configuration change and fresh size measurement. An
offline distribution may be added later if it can be labelled separately with its size and support
cost; it must not replace the small standard installer merely by implication. If the standard mode
changes away from `downloadBootstrapper`, update this contract and its test in the same change.
