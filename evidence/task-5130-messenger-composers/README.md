# TASK 5130 Messenger composer references

`live-census-2026-08-11.json` is the independently created census. It predates
the shipped contract and manifests and must never be regenerated from either
inventory.

The shipped key contract is
`../../tools/task-5102-carrier-reference/messenger-composer-contract.json`.
Its exact key is `origin/channel/composerState`, with origin
`https://www.messenger.com`, channels `direct-message`, `group`, and
`community`, and state `ordinary-unprotected-probe`. Every capture uses the
exact benign text `Task 5130 benign composer probe`.

The release checker is intentionally fail-closed and runs all provenance,
inventory, capture-count, rectangle, PNG-hash, seam-ring, and colour checks
before yielding a verified set to any visual diff:

```text
messenger-composer-reference-check <dated-live-census.json> <shipped-contract.json> <reviewed-manifest-directory>
```

Release manifests must use source kind `live-carrier-windows-release` and
capture method `windows-powershell-uia-copyfromscreen`. They record the signed
browser, independent profile/account, HWND generation, exact origin, Windows
UIA provider, `tasklist.exe`/Windows PowerShell/`CopyFromScreen` tools, and the
signed shipping build receipt. A unique marker must be present in the changed
carrier-visible after-state. The manifest also carries a dated, distinct-author
review receipt and signature. Fixture manifests use a separate source kind and
can only be accepted by the library's test-only fixture validation mode; the
shipping checker has no fixture-mode switch.

Each key has one reviewed manifest and exactly five referenced RGB/RGBA PNGs.
Each PNG contains only the exact UIA ROI and its four-pixel seam ring; the
absolute captured bounds must equal the ROI expanded by four pixels. The seam
hash must agree with the PNG and remain identical over the five captures. Each
PNG must contain at least 32 distinct RGB colours and meet its manifest's floor;
the ROI must contain more than two colours.
Unreferenced PNGs, reused PNGs, subdirectories, and non-JSON/non-PNG artifacts
make the reference store fail closed.

As recorded on 2026-08-11, the immutable census currently observed zero
Messenger composer states and no installed Messenger channels. Consequently,
release validation correctly exits 1 naming the first missing live-census
carrier state. No fixture, catalogue, overlay, canned response, or coordinated
contract/manifest shrink can turn that absence into a release pass.

The Windows-side producer is split deliberately. `scripts/qa/task-5130-messenger-census.ps1`
creates the contract-independent census. Only after that artifact exists may
`scripts/qa/task-5130-messenger-live-capture.ps1` read the contract, rebind each
UIA runtime id/profile/HWND, verify the signed shipping process and its per-key
marker/review receipt, and use `CopyFromScreen` for the exact five bounded
captures. Both scripts refuse ambiguity; neither has a catalogue or hidden
overlay mode.
