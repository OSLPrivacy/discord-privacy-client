# WhatsApp VM bootstrap

This bootstrap is limited to the dedicated `OSL-WhatsApp-Client-1/2` VMs. It uses Azure RunCommand only to arm an interactive, limited scheduled task for the exact `osltest` Windows session.

The task performs one operation:

- Verify the current user's exact `5319275A.WhatsAppDesktop_cv1g1gvanyjgm` Store package.
- If absent, install exact Microsoft Store product `9NKSQGP7F2NH` through the protected Desktop App Installer package, then reverify it.

It does not launch WhatsApp, link or unlink a device, inspect app storage, read a profile, stop a process, reboot, or expose secrets. A first device link remains a manual action by the user inside the VM.

After an `osltest` RDP session exists, the one-command controller is:

```bash
python3 scripts/qa/whatsapp/whatsapp-bootstrap-orchestrator.py \
  --vm OSL-WhatsApp-Client-1 \
  --invocation wa-bootstrap-0001
```

Use a fresh invocation ID for an independent audit record. Reusing the same ID with the same VM session is idempotent and returns `alreadyArmed` or `alreadyCompleted`.

This command was not run during implementation.
