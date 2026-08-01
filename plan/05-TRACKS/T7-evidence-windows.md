# T7 Windows evidence protocol

T7-02 may record verdicts for defects 12 and 13 only after T7-01 has produced an admissible
fresh-binary identity for the exact same source commit. The Windows run must use that binary in a
dedicated Windows VM and record observations through UI Automation, because capture protection
blocks screenshots.

Retain the following artifacts for a completed run:

- `build-identity.json` — the source commit, executable hash, and frontend build identity;
- `uia-status-tags.json` — UI Automation observations of neutral, warning, error, and success tags,
  including computed foreground/background/border values and border radius; and
- `run.log` — VM identity, viewport/DPI, launch command, and timestamp.

The evidence ledger remains `blocked` until those artifacts are available. A source-level CSS
inspection is not evidence for this task: the purpose of the re-shoot is to settle the Windows
rendering dispute.
