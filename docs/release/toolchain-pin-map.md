# Release and reproducible-build toolchain map

## Result

Both the release build and the reproducible-build run for `hub-v0.1.0` used
Rust `1.88.0` on `x86_64-pc-windows-msvc`. The four configured pins also agree
on `1.88.0`.

The release job checks out the tag before it installs Rust. The reproducible
job installs its default before resolving and detaching the release tag, but
the tag's `rust-toolchain.toml` is then the rustup directory override. Thus the
repro job must retain the tagged file's channel to rebuild that tag; its
earlier action input is only the installed/default toolchain.

## CI evidence

The release run [30649676776](https://github.com/OSLPrivacy/discord-privacy-client/actions/runs/30649676776)
and repro run [30658453427](https://github.com/OSLPrivacy/discord-privacy-client/actions/runs/30658453427)
each record the installed compiler below. The repro log additionally records
the active `rust-toolchain.toml` override before it detaches `hub-v0.1.0`; that
tag has the same channel. No Cargo command was run for this measurement.

```text
rustc 1.88.0 (6b00bc388 2025-06-23)
binary: rustc
commit-hash: 6b00bc3880198600130e1cf62b8f8a93494488cc
commit-date: 2025-06-23
host: x86_64-pc-windows-msvc
release: 1.88.0
LLVM version: 20.1.5
```

## Pin and tag map (measured 2026-07-31)

| Consumer | Pin / effective channel |
| --- | --- |
| `rust-toolchain.toml` | `1.88.0` |
| `.github/workflows/rust-test.yml` | `1.88.0` |
| `.github/workflows/osl-hub-release.yml` | `1.88.0` |
| `.github/workflows/reproducible-build.yml` | `1.88.0` (default; tag override wins after checkout) |
| `hub-v0.1.0` `rust-toolchain.toml` | `1.88.0` |

Run the guard before changing any of these pins:

```sh
python3 scripts/check_toolchain_pins.py
python3 -m unittest scripts/test_check_toolchain_pins.py
```

The guard reads every published `hub-v*` tag, verifies all four current pins,
and compares them to the recorded Windows `rustc -vV` release. If an old tag
needs a different channel after a future bump, it must stay in the map; T8-B6
owns the reproducible-build workflow change needed to preserve that history.
