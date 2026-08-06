# TASK 0025 Linux Screenshot Proof

Task-owned runtime artifacts for proving that the OSL desktop launcher can be
captured on a Linux virtual display with no physical monitor attached.

## Successful artifact

- Image: `out/osl-linux-xvfb-capture.png`
- Opened with the image viewer during the run: shows the OSL first-run screen
  with the OSL logo, `Create account`, and `Use recovery phrase`.
- Captured window: `OSL Privacy`
- Xvfb display: `:26`
- Virtual screen: `1440x900x24`
- Captured size: `1440x900`
- ImageMagick identify: `width=1440 height=900 colors=743 sha256=70a11213e6fb70dc62ee228e754770d0bf5ef29447ae6229592b4991a99672cb`
- File SHA-256: `190f75e98f1cab05963a899af32003a9076c79765f073f5bb586d13e230325b9`

## Failure artifact

After killing the OSL process, the same window-id capture command was run under
`timeout 5s`. It failed red:

```text
status=124
import-im6.q16: no window with specified ID exists `4194307': Resource temporarily unavailable @ error/xwindow.c/XImportImage/4887.
```

No after-kill PNG was saved.
