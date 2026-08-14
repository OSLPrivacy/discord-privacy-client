# Ground-ready command list

This is the compact command sheet for the build and QA handoff. Every command
has a saved green result and a saved red result in the cited audit evidence;
the red result is an intentional negative control, not an expected product
failure.

| Area | Green command and result | Red command and result | Saved evidence |
| --- | --- | --- | --- |
| One build | `CARGO_TARGET_DIR=/mnt/d/osl-lane-targets/c cargo test --manifest-path apps/osl-hub/Cargo.toml -p osl-hub --no-default-features --features core --test task_0043_carrier_machine_account_pairs -- --test-threads=1 --nocapture` → `carrier_claim_count=3`, `account_count=3`, `machine_count=3`, `discord_pair_count=3`, `1 passed; 0 failed` | Same command with a duplicate carrier account or machine in the fixture → assertion failure, non-zero exit | `0010`, `0020`, `0043` |
| Linux screen | `scripts/qa/test-linux-fixed-screen-launcher.sh` → `TWO_LAUNCHES_MATCH size=1280x800 scale=1 theme=dark` | Same check with the launcher shim reporting `1024x768` → `SIZE_CHANGE_CHECK_FAILED`, exit `70` | `0026`, `0066` |
| Two-copy | `scripts/qa/test-osl-two-copy-guide.sh` → `TASK0033 two-copy-guide live_status_count=2`, copies `A,B`, distinct live PIDs | `scripts/qa/osl-fast-two-person-test.sh --port-a 48267 --port-b 48267` → `osl-two-copy-startup: shared port: 48267`, exit `1`, zero status/metadata files | `0033`, `0067` |
| Azure | `python3 -m pytest -q docs/testing/test_machine_list.py --disable-warnings --maxfail=1` → `3 passed`; inventory → `azure_regions_checked=4`, processor counts `francecentral:2,italynorth:2,norwayeast:2,polandcentral:2` | Inventory with the Norway row removed → `norwayeast: expected 1 row, got 0`, exit `1` | `0038` |
| Attachment | Direct attachment-limit test with the real `25 * 1024 * 1024` free limit → `24 MB=accept`, `26 MB=reject`, `16 files=accept`, `17 files=reject`, `1 passed` | Same test with a temporary `26 * 1024 * 1024` free limit → `26 MB result=accept`, assertion `26 MB must be reject`, exit `101` | `0053` |
| Metadata | `VMQA_TEST_METADATA_DIR=/tmp/osl-0060 OSL_DISABLE_CSP_STRIP=1 scripts/vmqa/vmqa-run.sh test f1` followed by `python3 scripts/vmqa/read-test-result.py /tmp/osl-0060/unit-test-f1.json` → `accepted: version=0.0.1 switches=feature:desktop,OSL_DISABLE_CSP_STRIP=1`, exit `0` | Same reader on a record without `oneBuildVersion` → `refused: ... has no one-build version`, exit `1` | `0060`, `0065`, `0066` |

## Handoff rule

Each row must retain both result classes. A row missing either its saved green
result or its saved red result is not ground-ready.
