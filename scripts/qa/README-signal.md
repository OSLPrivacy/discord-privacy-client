# Signal Desktop two-VM QA scaffold

This lane is deliberately limited to two dedicated Signal VMs. It has no
browser route and does not reuse Discord or Telegram VM aliases, task names, or
working roots.

Build the deployed flavor with both compile-time and UI gates enabled:

    (cd apps/osl-hub-ui && VITE_OSL_SIGNAL_QA_SHELL=1 npm run build)
    TAURI_CONFIG="$(jq -c . apps/osl-hub/tauri.signal-qa.conf.json)" \
    cargo build --profile signal-qa --target x86_64-pc-windows-gnu \
      --features desktop,signal-qa-shell \
      --manifest-path apps/osl-hub/Cargo.toml

The `signal-qa-shell` backend creates one disposable QA identity using a random
device secret sealed by TPM or operating-system credential storage. It registers
only installed-app inventory, exact Signal window claim/focus/resize/detach, and
the read-only protected-send readiness receipt. No password, recovery, import,
setup, or unlock command is exposed to the renderer.
Home, browsers, Scrub, updates, mail, OSL Chats, attachments, and every other
provider command are absent from its Tauri IPC allowlist. The dedicated UI entry
does not import the general Hub renderer at all. Its final main bundle is audited
for excluded provider/route strings. The `signal-qa` Cargo profile uses whole-
program optimization and strips symbols. The separate Tauri configuration uses
the `org.oslprivacy.signalqa` storage identity and an empty updater endpoint list,
so it neither shares the production Hub namespace nor inherits its release feed.
Production builds are unchanged unless the feature, environment flag, profile,
and QA Tauri configuration are explicitly selected together.

The `signal_vm_qa.py run` command is the one-command
deploy/audit/redeploy foundation. It validates a credential-free manifest,
deploys hash-pinned OSL artifacts through
managed identity, replaces only the exact configured OSL process, verifies the
Signal PID set did not change, audits exact OSL and Signal hashes plus Signal's
Authenticode subject, lets the binary create/unlock its OS-sealed disposable QA
identity internally, captures a safe OSL-chrome image, then repeats the exact
deploy/audit cycle. Both deployment gates are terminal.
On a new VM, the deploy leaf permits only a fully empty configured OSL install
directory (both executable and loader absent), places the two pinned artifacts,
and rolls them back if launch fails. A half-present install fails closed.
Existing installs require exactly one matching primary process and use the
backup/replace/restore path; neither path deletes or rewrites a profile.

Copy signal-vm-qa.example.json outside the repository. Fill it only with
resource identifiers, exact paths and hashes, the observed valid Authenticode
subject, and private screenshot upload bases. Do not put passwords, secret
references, linking codes, phone
numbers, recovery material, tokens, cookies, QR data, or message content in it.

    python3 scripts/qa/signal_vm_qa.py validate --manifest /secure/path/signal-vm-qa.json
    python3 scripts/qa/signal_vm_qa.py plan --manifest /secure/path/signal-vm-qa.json
    python3 scripts/qa/signal_vm_qa.py run --manifest /secure/path/signal-vm-qa.json

The QA binary generates its device secret in memory, seals only that random
secret through OSL's existing TPM/OS credential sealer, and zeroizes recovery
material immediately. The secret and recovery material never enter automation,
the manifest, logs, receipts, or screenshots. Existing Signal state is never
used for this identity and remains untouched.

The interactive harness uses `PrintWindow` on the exact pinned OSL HWND, so it does
not depend on a foreground RDP session or capture desktop occlusion. The only
persisted pixels are the top 48 pixels of OSL's title bar. Signal, transcript,
composer, provider, attachment, and message regions are structurally excluded.
The PNG is uploaded to a private blob URI using managed identity and deleted
from the VM. The receipt records only its digest, dimensions, and safety enums.
This crop can prove background capture mechanics and lifecycle/title-bar polish;
it cannot prove transcript, composer, or provider-surface polish. Do not broaden
the crop until an independently reviewed mask can prove that every content pixel
is excluded.

The versioned `osl-vm-signal-uia-harness.ps1` is the source artifact referenced
by `build.harnessUri` and pinned by `build.harnessSha256`. It implements only
semantic `Inventory`, `ClaimExactWindow`, and `CaptureSafeChrome`. It verifies the exact Signal
process/window class/title and OSL's fixed `Claimed` status, returns bounded
counts/geometry, and never returns window titles, accessibility names, or
content. It never focuses either application.

The generated plan contains both directions for text, multiline/UTF-8,
encryption, protected composer, transcript overlay, burn, Covertext,
attachments/images, delivery/read receipts, reconnect, replay rejection,
malformed-data rejection, expiry, and window lifecycle. These 28 live stages are
marked `blockedUntilLiveAdapterReviewed`. The arm command exposes only
`Inventory`, `ClaimExactWindow`, and the titlebar-only `CaptureSafeChrome`;
unsupported actions cannot be selected.
This slice cannot send, read, burn, attach, or inspect messages until the exact
destination/composer adapter is reviewed against the signed installed build.

Receipts contain VM names, run/stage identifiers, timestamps, and status only.
Raw Azure output, Key Vault values, Signal profile data, UI text, and screenshot
bytes are not written to receipts.
