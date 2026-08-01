# T20 Linux boundary: window lifecycle and device plumbing

Status: scope boundary, not Linux evidence. This document records the work that
T20 deliberately does **not** claim to cover. It does not make the existing
Linux build a supported live-USB product.

## Decision that sets the boundary

Owner decision D62 puts a live-USB/amnesic Linux build in scope, while also
requiring its own track. The existing Linux build has run under Xvfb, so the
future work is packaging and persistence-off validation, **not a port**. That
does not transfer any of T20's Windows evidence to Linux.

The relevant trust ladder remains distinct:

| Environment | Must trust |
|---|---|
| Linux build | hardware + Linux + OSL |
| live USB, no persistence | hardware + OSL; nothing written to disk |

Hardware remains in the trust base in every configuration. A live USB changes
the host-OS and persistent-disk parts of the trust base; it does not establish
protection against hostile hardware.

## What T20 does not cover on Linux

T20's window-lifecycle matrix, measurements, and PowerShell VM harness are
Windows-only. In particular, T20 supplies no Linux evidence for:

- borrowed-window ownership, reparenting, restoring prior styles, z-order, or
  task-switcher behaviour. These checks use Windows HWND ownership and APIs
  such as `SetWindowLongPtrW` / `GWLP_HWNDPARENT`; there is no equivalent T20
  matrix for a Linux compositor;
- minimize, restore, focus, monitor, DPI, snap, black-surface repair, capture,
  or transition behaviour of a borrowed native window under X11 or Wayland;
- device-arrival/removal behaviour. T20's USB monitor is Windows notification
  plumbing; its non-Windows implementation is not evidence of Linux device
  monitoring or of a dead-man-switch guarantee;
- Azure Windows-VM screenshots, timings, verdict JSON, or any assertion that
  PowerShell harness results apply to Xvfb, a Linux desktop, or a live USB.

Xvfb/XTEST prior art is a useful starting point only. It is not coverage of
window-manager/compositor semantics, persistence-off behaviour, boot media, or
real hardware support.

## What the future live-USB track must cover

The future D62 track owns the product decision and evidence for Linux/live-USB
support: packaging and signed boot media, operation without persistent storage,
the chosen display-server and window-manager scope, device discovery/removal,
hardware and driver limits, and Linux-specific end-to-end QA. It must define
its own acceptance criteria and evidence; T20 contributes no Linux pass cells
to inherit.

This is a boundary record, **not a Linux test plan** and not an implementation
proposal. Until that track exists and produces evidence, describe Linux only as
an existing build that has run under Xvfb—not as a tested live-USB or amnesic
deployment.
