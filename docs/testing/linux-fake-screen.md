# Linux Fixed Screen Launcher

`scripts/qa/linux-fixed-screen-launcher.sh` starts an `Xvfb` display with a
fixed screen contract:

- size: `1280x800`
- colour depth: `24`
- window scale: `1`
- theme label: `dark`
- DPI: `96x96`

The launcher verifies the live server with `xdpyinfo` before it prints
`OSL_FIXED_SCREEN ...`. A server that starts with a different size, colour
depth, or DPI fails before any child command runs.

Run the contract test with:

```sh
scripts/qa/test-linux-fixed-screen-launcher.sh
```
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

Start the fixed OSL Linux screen launcher and print the launch contract:

```bash
scripts/qa/linux-fixed-screen-launcher.sh
```

It always starts the same fake screen contract for Linux QA:

```text
size=1280x800 color_depth=24 scale=1 theme=dark
```

Run a command inside that fixed screen:

```bash
CARGO_TARGET_DIR=/home/liamw/osl-exec-e/.cargo-target \
  scripts/qa/linux-fixed-screen-launcher.sh -- cargo test -- --test-threads=1
```

Check that two launches match and a wrong-size fake screen is rejected:

```bash
scripts/qa/test-linux-fixed-screen-launcher.sh
```

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
