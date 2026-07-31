# Signal Store installation contract

The Signal QA VMs install the official desktop client from the Microsoft Store listing with product ID `XP89119P9F2PCQ`. Installation is performed by winget with exact ID matching and the fixed `msstore` source. Browser downloads, direct installers, alternate sources, package search, and version guessing are outside this lane.

## Preconditions and boundary

- The dedicated Signal resource group and VM name must both contain `signal`; names containing Discord, Telegram, or WhatsApp are rejected.
- One Explorer process must identify the requested interactive session, and its owner must exactly match `osltest`.
- The installer runs as that interactive user in a limited scheduled task with a ten-minute execution limit.
- Signal must be closed before and after installation. The lane never launches Signal.
- The lane does not link an account, inspect a QR code, inspect Signal's profile/database, read message content, or use credentials.

## Exact install and evidence

The only allowed install command is semantically equivalent to:

```text
winget install --id XP89119P9F2PCQ --exact --source msstore --accept-package-agreements --accept-source-agreements --silent --disable-interactivity
```

After success, the runner checks only OSL's documented classic-app candidate at `%LOCALAPPDATA%\Programs\signal-desktop\Signal.exe`. It does not recursively search user storage or assume a Store package path. If that candidate is absent, the run fails closed for manual contract review.

The candidate must have a valid Authenticode signature and a nonempty file version. The terminal semantic receipt exposes only:

- version;
- SHA-256;
- Authenticode publisher subject;
- the constant path class `LocalAppDataProgramsSignalDesktop`.

The raw path and all private Signal state remain absent. The resulting hash and publisher are evidence to pin into the Signal QA manifest; they are not inferred in advance.
