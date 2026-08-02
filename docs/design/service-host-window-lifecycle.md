# Embedded service-host window lifecycle

This is the T20-D4 handoff to T4. It defines the geometry obligations for the
embedded `service-host` webview; it does not change `service_host.rs`, which is
owned by T3/T4.

## Re-layout triggers

T4 must call `ServiceHostState::set_layout` after a host is created and after
each of these events while an isolated service host is active:

- the main window is resized, moved, restored, snapped, tiled, or maximized;
- the main window's scale factor changes (`WM_DPICHANGED` / Tauri scale-change
  event);
- trusted-bar, local protected-sheet, or protected-mode state changes; and
- a host is replaced or its generation changes.

The renderer must wire this explicitly. `set_layout` is not a self-scheduling
layout loop, and a call only in tests is not a production lifecycle caller.

## Coordinate contract

`current_host_rect` and `current_host_rect_with_local_sheet` obtain physical
window size from Tauri, convert exactly once using the current `scale_factor`,
then pass logical coordinates to Tauri. Do not cache logical coordinates across
a scale-factor event and do not convert the resulting logical rectangle a
second time. A relayout after every scale change is required, including a
non-integer scale such as 125% or 150%.

The host rectangle must remain fully inside the current main-window client
area. If a fresh scale factor or client size cannot be obtained, fail the
layout and retain no stale, potentially off-screen bounds.

## Top reserve

The vertical origin is always the current `DESKTOP_TITLE_HEIGHT` plus
`TRUSTED_BAR_HEIGHT`, expressed in logical pixels after the one conversion
above. `DESKTOP_TITLE_HEIGHT` is currently zero; it remains a reserve in the
formula so a platform title region can be restored without an overlapping
host. Under non-integer scaling, derive the reserve from the logical geometry
calculation rather than rounding the physical title and trusted-bar heights
separately.

## Verification handoff

T4 must extend the T20-D2 mixed-DPI proof to exercise this embedded substrate:
move the main window over a DPI boundary, force the wired relayout, and compare
the physical webview bounds with the post-transition `winstate` snapshot. The
test must prove that the host has neither a gap under the trusted bar nor a
one-scale-factor offset.
