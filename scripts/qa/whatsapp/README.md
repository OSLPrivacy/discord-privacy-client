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

## Metadata-only UIA structure probe

`whatsapp-uia-probe-orchestrator.py` arms an independently hash-pinned,
limited-user scheduled task in the exact `osltest` session. The task attaches
only when one exact official Store package, process, and main window exist.

It records only bounded structural metadata needed to review a future selector
contract: control type, strictly screened automation/class/framework tokens,
salted runtime hashes, quantized relative geometry, state flags, and structural
parent hashes. It never reads UIA names, values, text/help properties, provider
storage, private APIs, or content; and it never clicks, types, sends, launches,
foregrounds, or terminates anything. Ambiguity and UI tree instability fail
closed. The output is capped at 512 nodes, depth 12, 256 KiB, and 15 seconds.

After WhatsApp is manually open in the dedicated VM's `osltest` session:

```bash
python3 scripts/qa/whatsapp/whatsapp-uia-probe-orchestrator.py \
  --vm OSL-WhatsApp-Client-1 \
  --invocation wa-uia-0001
```

This command was not run during implementation.
