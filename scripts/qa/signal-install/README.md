# Signal Microsoft Store install lane

This lane installs only Microsoft Store product `XP89119P9F2PCQ` from the fixed `msstore` winget source. It requires the exact `osltest` interactive session to exist before arming a limited scheduled task.

```powershell
python scripts/qa/signal-install/install_signal_store.py `
  --resource-group OSL-TWO-CLIENT-LAB-SIGNAL `
  --vm-name OSL-Signal-Client-1 `
  --session-id 2 `
  --windows-user osltest `
  --receipt-dir .\signal-install-receipts
```

The command never explicitly launches or links Signal, never inspects its
profile, and accepts no secrets. The Store installer can auto-launch the app;
when that happens the lane leaves it running and verifies every resulting
`Signal` process resolves to the exact signed candidate. The receipt contains
only Store identity, version, SHA-256, publisher subject, path class, and a
bounded process-state enum.

If installation already completed (including an installer auto-launch), recover
the terminal evidence without reinstalling or closing Signal:

```powershell
python scripts/qa/signal-install/install_signal_store.py `
  --resource-group OSL-SIGNAL-QA-SCUS `
  --vm-name OSL-Signal-Client-1 `
  --session-id 2 `
  --windows-user osltest `
  --receipt-dir .\signal-install-receipts `
  --audit-only
```

Copy evidence into the QA manifest only after both dedicated VMs match.
