# Linux Fake-Screen Window Capture

Use Xvfb for a fake Linux X11 screen and ImageMagick for one-window capture.
These tools are for Linux QA only; they do not change the Windows-only
`WDA_EXCLUDEFROMCAPTURE` capture-resistance path.

## Packages

Debian/Ubuntu:

```bash
sudo apt-get update
sudo apt-get install -y xvfb x11-utils xdotool imagemagick
```

Optional local smoke-test window:

```bash
sudo apt-get install -y x11-apps
```

## Commands

Start a fake screen and print its display name:

```bash
scripts/qa/linux-fake-screen.sh
```

Run OSL on the fake screen and capture one matching window:

```bash
CARGO_TARGET_DIR=/home/liamw/osl-exec-e/.cargo-target \
  scripts/qa/linux-fake-screen.sh \
    --capture-window 'OSL Privacy' \
    --output /tmp/osl-window.png \
    -- cargo tauri dev \
      --manifest-path apps/osl-hub/Cargo.toml \
      --features desktop
```

The helper monitors the Xvfb server while the child command runs. If that fake
screen is stopped before the command exits, the helper exits nonzero.
