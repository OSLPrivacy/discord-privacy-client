# Windows window-lifecycle baseline

Status: **not measured; no lifecycle cell is a pass.** This record is the
baseline result for T20-B5 as run on 2026-08-02. It deliberately reports an
evidence gap instead of converting source-level expectations into VM results.

## Execution target and attempted evidence collection

The assigned target is `osl-client-4` in Azure resource group `OSL-VMQA`
(`spaincentral`, private address `10.0.0.5`, public address
`158.158.49.24`). Azure reported it `VM running` at collection time.

The required collection programs are not present in the tracked harness:

| required input | expected by | observed in `vmqa/` |
|---|---|---|
| `winstate.ps1` | T20-B1 / TW-0 | absent |
| `winstate-transient.ps1` | T20-B2 / TW-0 | absent |
| transition driver and per-substrate launch fixtures | T20-B5 | absent |
| JSON snapshots and PNGs for the 75 cells | T20-B5 | absent (`vmqa/results/` contains only `.gitkeep`) |

The only runnable harness artifact is the deliberately failing
`selftest-neg.ps1`; it proves the negative-control reporting path, not a
window-lifecycle transition. No run was sent to the VM because without B1 and
B2 it could neither identify an HWND nor record a short-lived popup. A
screenshot without the associated `winstate` record is explicitly not
evidence for this matrix.

## Required recording format once B1/B2 land

For every cell, retain one `winstate` JSON object per top-level OSL or
borrowed-process HWND at transition start, throughout the transition, and at
settlement. Each object must contain class, style, ex-style,
`GWLP_HWNDPARENT`, rect, `IsIconic`, `IsWindowVisible`, z-order index,
`GetDpiForWindow`, and `MonitorFromWindow` id. Run the transient enumerator at
50 ms for transitions 1–15 and retain every HWND that appears and dies. Store
one screenshot with each cell, but score the cell from the JSON, not pixels.

The recording command is not prescribed here because its B1/B2 scripts do not
exist yet. When they do, archive the command line, app build hash, Windows
build, monitor topology and DPI values beside the JSON/PNG pair. Re-run this
document from an empty results directory; do not overwrite or merge old
captures.

## Matrix result

`UNMEASURED` means exactly that no live HWND/PNG evidence exists. It does not
mean the code path is expected to work or fail.

| transition | S-NAT | S-MUL | S-BRW | S-EMB | S-OVL |
|---|---|---|---|---|---|
| 1 move (caption drag) | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 2 resize (edge drag) | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 3 maximise / restore | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 4 minimise / restore | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 5 Win+arrow snap / tiling | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 6 second monitor, same DPI | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 7 second monitor, different DPI | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 8 live display-scale change | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 9 monitor unplug while borrowed | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 10 alt-tab away / back | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 11 OSL loses focus to third app | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 12 OSL closes with child attached | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 13 borrowed app quits | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 14 borrowed app modal / popup | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |
| 15 z-order fight | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED | UNMEASURED |

Substrates are the track definitions: S-NAT native borrow, S-MUL Mullvad
Electron borrow, S-BRW borrowed browser, S-EMB embedded WebView2 host, and
S-OVL decrypted overlay.

## Release implication

Groups C–E must not use this record to claim a fixed cell or choose a fix.
The next action is to deliver T20-B1 and T20-B2, then capture all 75 cells on
this same VM. In particular, live DPI change (transition 8) remains untested;
moving a window between monitors is not a substitute.
