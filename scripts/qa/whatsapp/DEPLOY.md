# Immutable WhatsApp QA pair deployment

`whatsapp-deploy-orchestrator.py` deploys one already-uploaded immutable OSL
build to both dedicated WhatsApp VMs in one command. It does not upload an
artifact or accept a SAS URL. Each VM downloads the two query-free blobs from
the fixed storage account with its system-assigned managed identity and verifies
the supplied SHA-256 values before installation.

```bash
python3 scripts/qa/whatsapp/whatsapp-deploy-orchestrator.py \
  --invocation wa-deploy-0001 \
  --exe-uri https://osltestartifactsa7d5.blob.core.windows.net/BUILD/OSL%20Privacy.exe \
  --exe-sha256 EXACT_64_HEX_SHA256 \
  --webview2-loader-uri https://osltestartifactsa7d5.blob.core.windows.net/BUILD/WebView2Loader.dll \
  --webview2-loader-sha256 EXACT_64_HEX_SHA256
```

Both blob URLs must share the same build directory. The controller permits only
`OSL-WhatsApp-Client-1/2` in `OSL-WHATSAPP-TWO-CLIENT-LAB` and uses Azure
RunCommand for every guest operation. It discovers the exact `osltest` session,
then the guest leaf:

- verifies the official Store WhatsApp registration without reading its private
  storage;
- snapshots the package-owned process set and top-level window state;
- stages and hashes both OSL artifacts;
- stops only the exact OSL primary and waits for its WhatsApp guardian to exit;
- atomically replaces both installed files and launches OSL as the limited
  interactive user;
- rejects any WhatsApp process/window lifecycle change;
- restores and relaunches the previous exact OSL build on failure; and
- emits a redacted semantic receipt, written atomically with mode `0600`.

Successful backups and staging files are removed, so deploying the same pinned
build again is an idempotent audit. The scripts never reboot, launch or close
WhatsApp, foreground a window, use a browser fallback, inspect WhatsApp profile
files, or stop an unrelated process.

These commands were not run during implementation.
