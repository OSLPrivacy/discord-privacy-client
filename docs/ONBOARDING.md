# Onboarding

This page describes the onboarding flow that the desktop app builds today. It
is a route reference for product, support, and copy work; the UI implementation
lives in `apps/osl-hub-ui/src/`.

## How setup starts

On first launch, OSL opens the welcome screen. From there, a person can create
an account, restore an account from an identity recovery phrase, or unlock an
account that is already set up on this device. There is no sign-in to a third-
party service during setup.

After an account is created or restored, OSL shows the recovery kit before the
optional setup choices. If the recovery kit has not been confirmed as saved,
OSL returns to it after a restart instead of silently skipping it.

## Setup path

The normal path after the recovery kit is:

1. Enter or skip a Pro activation code.
2. Choose a protection level.
3. Review the starting privacy and cleanup defaults.
4. Choose how OSL prepares a protected message for sending.
5. Review cover-text insertion.
6. Optionally set a stealth password and a burn password.
7. Optionally use or install Mullvad.
8. Optionally inspect saved browser accounts, choose apps, install missing
   apps, and connect the apps selected during setup.

Choices are optional where the screen offers **Skip** or **Not now**. Setup
does not log in to, launch, or modify a selected app merely because it appears
in the chooser.

## Route reference

The table lists every route currently rendered by the onboarding shell. Routes
with a slash are alternatives, not consecutive screens.

| Route | What the person sees |
| --- | --- |
| `welcome` | Start screen with account creation, account restoration, or device unlock. |
| `create` | Password creation for a new account. |
| `import` | Identity recovery phrase entry and a new password for a restored account. |
| `unlock` | Password entry for an existing account on this device. |
| `recovery` | Recovery kit display and its save acknowledgement before optional setup. |
| `pro` | Optional Pro activation code. |
| `privacy` | Basic, Balanced, or Maximum protection choice. Balanced is the recommended starting point. |
| `defaults` | Review of starting warnings, attachment cleaning, retained drafts, cleanup, and send behavior. |
| `sending` | Screen-capture preference and a choice of manual, clipboard, or Double Enter preparation. OSL does not silently send. |
| `cover` | Cover insertion comparison: free insertion on send and the pending Pro typing option. |
| `passwords` | Optional stealth password, which opens an empty workspace without private data. |
| `burnpass` | Optional burn password, which erases OSL data on this device when entered at sign-in. |
| `mullvad` | Optional Mullvad setup or launch when it is available on Windows. |
| `browser` | Optional, consented search for saved browser accounts. OSL does not inspect browser data before consent. |
| `tutorial` | Choice of available apps to show on Home. |
| `detected` | Use already-detected desktop apps. |
| `install` | Optional installation of missing desktop apps through Windows. |
| `apps` | Connection of each selected app, with a per-app **Not now** choice. |
| `decoy` | Empty workspace shown after a stealth-password sign-in; it is an outcome, not a first-launch step. |

## Important limits shown during setup

- Screen-capture resistance is a Windows feature for ordinary screenshots and
  recording when Windows supports it. It cannot stop cameras, malware, or a
  modified device.
- A burn password clears OSL-owned local data. It does not erase data already
  received by someone else, original third-party-app data, or operating-system
  backups.
- Browser account discovery is optional and consented. OSL does not read
  browser databases before that consent.
- Sending modes prepare protected text but do not silently send it. If OSL
  cannot verify the destination, it copies the text and sends nothing.

For broader product limitations, see [THREAT_MODEL.md](THREAT_MODEL.md).
