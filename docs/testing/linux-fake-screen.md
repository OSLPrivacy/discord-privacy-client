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
