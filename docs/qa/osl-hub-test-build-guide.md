# OSL Hub test build guide

Use one OSL Hub executable for test runs. Test-only behavior is selected at
process startup with `OSL_TEST_ONLY_RUNTIME_SWITCHES`, so the executable under
test stays the same build.

## Build command

```powershell
Push-Location apps\osl-hub
tauri build --features desktop
Pop-Location
```

## Test switches

Set the switches on the process that launches the built executable:

```powershell
$env:OSL_TEST_ONLY_RUNTIME_SWITCHES = "password_screen_access=skip-password-screen-for-test safe_sending=dry-run-send-for-test"
.\target\release\osl-privacy-hub.exe
```

The safe default set is used when `OSL_TEST_ONLY_RUNTIME_SWITCHES` is unset.
Keep test procedures on this one build command; do not add a separate
feature-selected test executable.
