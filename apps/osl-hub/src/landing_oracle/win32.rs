//! The live Windows judges.
//!
//! Nothing in here decides anything. It walks, reads and samples, and every
//! cross-process call runs on a worker thread with its own COM apartment so a
//! provider that stops answering costs a parked thread rather than a frozen
//! OSL — the discipline `native_a11y::win32::bounded` established, and the one
//! `native_window_host.rs:5138-5160` records the cost of skipping (an unbounded
//! MSAA walk wedged the app live).
//!
//! **There is no write verb in this file.** No `SetValue`, no `SendInput`, no
//! `Invoke`, no `PostMessage`. The judge cannot move what it measures.

use super::{
    BoundComposer, Ink, JudgeDeadline, JudgeTimeout, LandingJudgeSyscalls, Rect, RenderedDocument,
    WalkCaps, WindowIdentity,
};
use crate::native_a11y::{call_with_timeout, Uia2TreeRoute};

use ::windows::core::{Interface, VARIANT};
use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, IDispatch, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED,
};
use ::windows::Win32::System::Ole::{
    SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
    SafeArrayUnaccessData,
};
use ::windows::Win32::UI::Accessibility::{
    AccessibleChildren, CUIAutomation, IAccessible, IUIAutomation, IUIAutomationElement,
    IUIAutomationLegacyIAccessiblePattern, IUIAutomationTextPattern, IUIAutomationTreeWalker,
    IUIAutomationValuePattern, TreeScope_Subtree, UIA_LegacyIAccessiblePatternId,
    UIA_TextPatternId, UIA_ValuePatternId,
};
use std::ffi::c_void;

/// The largest rectangle the ink sampler will read, in pixels. A composer is a
/// strip; anything larger than this is not one, and reading it would be an
/// unbounded allocation driven by another process's geometry.
const MAX_INK_PIXELS: usize = 4_000_000;

/// How far a pixel must sit from the rectangle's modal colour to be a glyph.
/// Per channel, 0-255.
const INK_THRESHOLD: i32 = 24;

pub(crate) struct LandingJudgeWin32;

struct ComGuard(bool);

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

fn bounded<T: Send + 'static>(
    deadline: JudgeDeadline,
    call: impl FnOnce() -> T + Send + 'static,
) -> Result<T, JudgeTimeout> {
    call_with_timeout(deadline.millis(), move || {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        call()
    })
    .map_err(|timeout| JudgeTimeout {
        millis: timeout.timeout_ms,
    })
}

fn automation() -> Option<IUIAutomation> {
    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()
}

/// The element the route says is the root of this window's tree. Deliberately
/// the same two arms as `native_a11y::win32::tree_root`: a judge that resolved
/// the composer by a different route than the writer did would be measuring a
/// different window, not a different channel.
fn tree_root(
    automation: &IUIAutomation,
    hwnd: isize,
    route: Uia2TreeRoute,
) -> Option<IUIAutomationElement> {
    match route {
        Uia2TreeRoute::UiaNative => unsafe { automation.ElementFromHandle(HWND(hwnd as _)) }.ok(),
        Uia2TreeRoute::MsaaBridge => {
            let accessible = crate::native_a11y::wake_electron_accessibility(hwnd)?;
            crate::native_a11y::element_from_ia_accessible(automation, &accessible).ok()
        }
    }
}

fn runtime_id_of(element: &IUIAutomationElement) -> Vec<i32> {
    let Ok(array) = (unsafe { element.GetRuntimeId() }) else {
        return Vec::new();
    };
    if array.is_null() {
        return Vec::new();
    }
    let values = (|| {
        let lower = unsafe { SafeArrayGetLBound(array, 1) }.ok()?;
        let upper = unsafe { SafeArrayGetUBound(array, 1) }.ok()?;
        if upper < lower || upper - lower > 64 {
            return None;
        }
        let mut data: *mut c_void = std::ptr::null_mut();
        unsafe { SafeArrayAccessData(array, &mut data) }.ok()?;
        let length = (upper - lower + 1) as usize;
        let values = if data.is_null() {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(data.cast::<i32>(), length) }.to_vec()
        };
        let _ = unsafe { SafeArrayUnaccessData(array) };
        Some(values)
    })()
    .unwrap_or_default();
    let _ = unsafe { SafeArrayDestroy(array) };
    values
}

/// Re-find the composer under the bound window's root. Runs on every judgement:
/// a `IUIAutomationElement` is apartment-bound and cannot be cached across
/// calls, and re-finding is also what makes the window bind load-bearing rather
/// than decorative.
fn composer_element(
    automation: &IUIAutomation,
    bound: &BoundComposer,
) -> Option<IUIAutomationElement> {
    let root = tree_root(automation, bound.hwnd, bound.route)?;
    let condition = unsafe { automation.CreateTrueCondition() }.ok()?;
    let found = unsafe { root.FindAll(TreeScope_Subtree, &condition) }.ok()?;
    let length = unsafe { found.Length() }.ok()?;
    (0..length)
        .filter_map(|index| unsafe { found.GetElement(index) }.ok())
        .find(|element| runtime_id_of(element) == bound.composer.runtime_id)
}

/// Put the keyboard focus on the composer and **prove it took it**.
///
/// This is **not** part of [`LandingJudgeSyscalls`] and never will be: `SetFocus`
/// moves provider state, and a judge that could move what it measures is the
/// defect this module exists to remove. It lives here only because a probe that
/// injects `SendInput` must have a focus proof, and it returns `false` rather
/// than assuming.
pub(crate) fn focus_composer(bound: &BoundComposer) -> bool {
    let bound = bound.clone();
    let took_focus = call_with_timeout(2_000, move || {
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard(initialized.is_ok());
        let Some(automation) = automation() else {
            return false;
        };
        let Some(element) = composer_element(&automation, &bound) else {
            return false;
        };
        if unsafe { element.SetFocus() }.is_err() {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        // Re-find: focusing can rebuild the subtree.
        let Some(element) = composer_element(&automation, &bound) else {
            return false;
        };
        unsafe { element.CurrentHasKeyboardFocus() }
            .map(|value| value.as_bool())
            .unwrap_or(false)
    });
    took_focus.unwrap_or(false)
}

// ---------------------------------------------------------------------------
// J1 — the rendered document through UIA
// ---------------------------------------------------------------------------

struct LeafWalk {
    leaves: Vec<String>,
    nodes_visited: usize,
    depth_reached: usize,
    /// Set when a cap was reached. A bound reached is a refusal, so the caller
    /// must be able to tell "256 nodes" from "256 nodes and more waiting".
    hit_cap: bool,
}

fn walk_leaves_uia(
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    caps: WalkCaps,
    depth: usize,
    out: &mut LeafWalk,
) {
    if out.nodes_visited >= caps.max_nodes || depth > caps.max_depth {
        out.hit_cap = true;
        return;
    }
    out.depth_reached = out.depth_reached.max(depth);

    let first = unsafe { walker.GetFirstChildElement(element) }.ok();
    match first {
        None => {
            // A leaf. Chromium publishes a static text node's content in the
            // name, which is why the read is `CurrentName` and not the value
            // property this module disowns.
            let name = unsafe { element.CurrentName() }
                .map(|name| name.to_string())
                .unwrap_or_default();
            if !name.is_empty() {
                out.leaves.push(name);
            }
            out.nodes_visited += 1;
        }
        Some(mut child) => loop {
            out.nodes_visited += 1;
            if out.nodes_visited >= caps.max_nodes {
                out.hit_cap = true;
                return;
            }
            walk_leaves_uia(walker, &child, caps, depth + 1, out);
            if out.hit_cap {
                return;
            }
            match unsafe { walker.GetNextSiblingElement(&child) } {
                Ok(next) => child = next,
                Err(_) => break,
            }
        },
    }
}

// ---------------------------------------------------------------------------
// J2 — the same document through Chromium's MSAA provider
// ---------------------------------------------------------------------------

fn msaa_self() -> VARIANT {
    VARIANT::from(0i32)
}

/// Chromium puts a static text node's content in `accName` and an editable
/// node's in `accValue`, so a walk over a Slate document has to be willing to
/// read either — the rule `native_discord_adapter.rs:11889-11893` states.
fn msaa_text_of(node: &IAccessible, child: &VARIANT) -> String {
    let name = unsafe { node.get_accName(child) }
        .map(|name| name.to_string())
        .unwrap_or_default();
    if !name.is_empty() {
        return name;
    }
    unsafe { node.get_accValue(child) }
        .map(|value| value.to_string())
        .unwrap_or_default()
}

/// One node in the MSAA walk: either a full object, or a simple child that can
/// only be read through its container.
struct MsaaNode {
    reader: IAccessible,
    child: VARIANT,
    walkable: bool,
}

/// The direct children of one MSAA container, bounded. A count above the cap is
/// refused outright rather than truncated: a partial child list would silently
/// drop part of the document, and a proof built on a partial list is not a
/// proof.
fn msaa_children(container: &IAccessible, limit: usize) -> Option<Vec<MsaaNode>> {
    let count = unsafe { container.accChildCount() }.ok()?;
    let count = usize::try_from(count).ok()?;
    if count > limit {
        return None;
    }
    if count == 0 {
        return Some(Vec::new());
    }
    let mut variants = vec![VARIANT::default(); count];
    let mut obtained = 0i32;
    unsafe { AccessibleChildren(container, 0, &mut variants, &mut obtained) }.ok()?;
    let obtained = usize::try_from(obtained).ok()?.min(count);
    variants.truncate(obtained);
    Some(
        variants
            .into_iter()
            .map(|variant| {
                if let Ok(object) = IDispatch::try_from(&variant)
                    .and_then(|dispatch| dispatch.cast::<IAccessible>())
                {
                    return MsaaNode {
                        reader: object,
                        child: msaa_self(),
                        walkable: true,
                    };
                }
                MsaaNode {
                    reader: container.clone(),
                    child: variant,
                    walkable: false,
                }
            })
            .collect(),
    )
}

fn walk_leaves_msaa(node: &MsaaNode, caps: WalkCaps, depth: usize, out: &mut LeafWalk) {
    if out.nodes_visited >= caps.max_nodes || depth > caps.max_depth {
        out.hit_cap = true;
        return;
    }
    out.depth_reached = out.depth_reached.max(depth);
    out.nodes_visited += 1;

    let children = if node.walkable {
        match msaa_children(&node.reader, caps.max_nodes) {
            Some(children) => children,
            None => {
                out.hit_cap = true;
                return;
            }
        }
    } else {
        Vec::new()
    };

    if children.is_empty() {
        let text = msaa_text_of(&node.reader, &node.child);
        if !text.is_empty() {
            out.leaves.push(text);
        }
        return;
    }
    for child in &children {
        walk_leaves_msaa(child, caps, depth + 1, out);
        if out.hit_cap {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// J3 — ink
// ---------------------------------------------------------------------------

fn element_rect(element: &IUIAutomationElement) -> Option<Rect> {
    let rect = unsafe { element.CurrentBoundingRectangle() }.ok()?;
    let rect = Rect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    };
    (!rect.is_degenerate()).then_some(rect)
}

/// Sample a screen rectangle and count the pixels that are not its own modal
/// colour.
///
/// A desktop-DC `BitBlt`, not `PrintWindow`: Chromium composites on the GPU and
/// `PrintWindow` answers with a flat colour for exactly the surfaces that
/// matter here (`native_window_host.rs:4172-4235` uses that fact to *prove*
/// capture protection). Reading the desktop means the answer is what a person
/// looking at the screen would see, which is the claim the oracle is making.
fn ink_in_rect(rect: Rect) -> Option<Ink> {
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };

    let width = rect.width();
    let height = rect.height();
    if width <= 0 || height <= 0 {
        return None;
    }
    let pixels = (width as usize).checked_mul(height as usize)?;
    if pixels > MAX_INK_PIXELS {
        return None;
    }

    unsafe {
        let screen = GetDC(std::ptr::null_mut());
        if screen.is_null() {
            return None;
        }
        let memory = CreateCompatibleDC(screen);
        if memory.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen);
            return None;
        }
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if bitmap.is_null() {
            DeleteDC(memory);
            ReleaseDC(std::ptr::null_mut(), screen);
            return None;
        }
        let previous = SelectObject(memory, bitmap as _);
        let copied = BitBlt(
            memory, 0, 0, width, height, screen, rect.left, rect.top, SRCCOPY,
        );

        let mut buffer = vec![0u32; pixels];
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top-down, so row order matches the screen.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB as u32,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        };
        let read = GetDIBits(
            memory,
            bitmap,
            0,
            height as u32,
            buffer.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        );

        SelectObject(memory, previous);
        DeleteObject(bitmap as _);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);

        if copied == 0 || read == 0 {
            return None;
        }

        // The modal colour, quantised to 5 bits per channel so anti-aliasing
        // does not split the background across dozens of buckets.
        let mut histogram = std::collections::HashMap::<u32, u32>::new();
        for pixel in &buffer {
            let key = ((pixel >> 3) & 0x1f)
                | (((pixel >> 11) & 0x1f) << 5)
                | (((pixel >> 19) & 0x1f) << 10);
            *histogram.entry(key).or_default() += 1;
        }
        let modal = histogram
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(key, _)| key)
            .unwrap_or(0);
        let modal_b = ((modal & 0x1f) << 3) as i32;
        let modal_g = (((modal >> 5) & 0x1f) << 3) as i32;
        let modal_r = (((modal >> 10) & 0x1f) << 3) as i32;

        let mut inked = 0u32;
        for pixel in &buffer {
            let b = (pixel & 0xff) as i32;
            let g = ((pixel >> 8) & 0xff) as i32;
            let r = ((pixel >> 16) & 0xff) as i32;
            let distance = (b - modal_b)
                .abs()
                .max((g - modal_g).abs())
                .max((r - modal_r).abs());
            if distance > INK_THRESHOLD {
                inked += 1;
            }
        }

        Some(Ink {
            rect,
            sampled: pixels as u32,
            inked,
        })
    }
}

// ---------------------------------------------------------------------------
// The seam
// ---------------------------------------------------------------------------

impl LandingJudgeSyscalls for LandingJudgeWin32 {
    fn window_identity(
        &self,
        hwnd: isize,
        deadline: JudgeDeadline,
    ) -> Result<WindowIdentity, JudgeTimeout> {
        bounded(deadline, move || {
            use windows_sys::Win32::Foundation::CloseHandle;
            use windows_sys::Win32::System::Threading::{
                OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
            };
            use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

            let mut process_id = 0u32;
            unsafe { GetWindowThreadProcessId(hwnd as _, &mut process_id) };
            let mut process_name = String::new();
            if process_id != 0 {
                let handle =
                    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
                if !handle.is_null() {
                    let mut buffer = [0u16; 512];
                    let mut length = buffer.len() as u32;
                    let ok = unsafe {
                        QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut length)
                    };
                    unsafe { CloseHandle(handle) };
                    if ok != 0 {
                        let path = String::from_utf16_lossy(&buffer[..length as usize]);
                        process_name = path
                            .rsplit(['\\', '/'])
                            .next()
                            .unwrap_or(&path)
                            .trim_end_matches(".exe")
                            .trim_end_matches(".EXE")
                            .to_owned();
                    }
                }
            }
            WindowIdentity {
                hwnd,
                process_id,
                process_name,
            }
        })
    }

    fn rendered_document_uia(
        &self,
        bound: &BoundComposer,
        caps: WalkCaps,
        join: &str,
        deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        let bound = bound.clone();
        let join = join.to_owned();
        bounded(deadline, move || {
            let automation = automation()?;
            let element = composer_element(&automation, &bound)?;
            let walker = unsafe { automation.RawViewWalker() }.ok()?;
            let mut walk = LeafWalk {
                leaves: Vec::new(),
                nodes_visited: 0,
                depth_reached: 0,
                hit_cap: false,
            };
            walk_leaves_uia(&walker, &element, caps, 0, &mut walk);
            if walk.hit_cap {
                // Report the cap as reached so the oracle refuses rather than
                // believing a truncated read.
                return Some(RenderedDocument {
                    text: walk.leaves.join(&join),
                    leaves: walk.leaves,
                    nodes_visited: caps.max_nodes,
                    depth_reached: caps.max_depth,
                });
            }
            if walk.leaves.is_empty() {
                // No text leaf at all. Never reported as "empty".
                return None;
            }
            Some(RenderedDocument {
                text: walk.leaves.join(&join),
                leaves: walk.leaves,
                nodes_visited: walk.nodes_visited,
                depth_reached: walk.depth_reached,
            })
        })
    }

    fn rendered_document_text_pattern(
        &self,
        bound: &BoundComposer,
        caps: WalkCaps,
        deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        let bound = bound.clone();
        bounded(deadline, move || {
            let automation = automation()?;
            let element = composer_element(&automation, &bound)?;
            let pattern = unsafe { element.GetCurrentPattern(UIA_TextPatternId) }
                .ok()?
                .cast::<IUIAutomationTextPattern>()
                .ok()?;
            let range = unsafe { pattern.DocumentRange() }.ok()?;
            // Bounded by the same node cap, read as characters: an unbounded
            // GetText would let another process's document size decide this
            // allocation.
            let text = unsafe { range.GetText(caps.max_nodes as i32) }
                .ok()?
                .to_string();
            Some(RenderedDocument {
                leaves: vec![text.clone()],
                text,
                nodes_visited: 1,
                depth_reached: 0,
            })
        })
    }

    fn rendered_document_msaa(
        &self,
        bound: &BoundComposer,
        caps: WalkCaps,
        join: &str,
        deadline: JudgeDeadline,
    ) -> Result<Option<RenderedDocument>, JudgeTimeout> {
        let bound = bound.clone();
        let join = join.to_owned();
        bounded(deadline, move || {
            let automation = automation()?;
            let element = composer_element(&automation, &bound)?;
            // Bridge down from the UIA element to Chromium's own IAccessible
            // for the SAME node. A second COM interface, a second marshalling
            // path, over the same layout tree.
            let legacy = unsafe { element.GetCurrentPattern(UIA_LegacyIAccessiblePatternId) }
                .ok()?
                .cast::<IUIAutomationLegacyIAccessiblePattern>()
                .ok()?;
            let accessible = unsafe { legacy.GetIAccessible() }.ok()?;
            let mut walk = LeafWalk {
                leaves: Vec::new(),
                nodes_visited: 0,
                depth_reached: 0,
                hit_cap: false,
            };
            let root = MsaaNode {
                reader: accessible,
                child: msaa_self(),
                walkable: true,
            };
            walk_leaves_msaa(&root, caps, 0, &mut walk);
            if walk.leaves.is_empty() {
                return None;
            }
            Some(RenderedDocument {
                text: walk.leaves.join(&join),
                leaves: walk.leaves,
                nodes_visited: walk.nodes_visited,
                depth_reached: walk.depth_reached,
            })
        })
    }

    fn composer_ink(
        &self,
        bound: &BoundComposer,
        deadline: JudgeDeadline,
    ) -> Result<Option<Ink>, JudgeTimeout> {
        let bound = bound.clone();
        bounded(deadline, move || {
            let automation = automation()?;
            let element = composer_element(&automation, &bound)?;
            let rect = element_rect(&element)?;
            ink_in_rect(rect)
        })
    }

    fn disowned_value_property(
        &self,
        bound: &BoundComposer,
        deadline: JudgeDeadline,
    ) -> Result<Option<String>, JudgeTimeout> {
        let bound = bound.clone();
        bounded(deadline, move || {
            let automation = automation()?;
            let element = composer_element(&automation, &bound)?;
            let pattern = unsafe { element.GetCurrentPattern(UIA_ValuePatternId) }
                .ok()?
                .cast::<IUIAutomationValuePattern>()
                .ok()?;
            unsafe { pattern.CurrentValue() }
                .ok()
                .map(|value| value.to_string())
        })
    }

    fn submit_shaped_calls(&self) -> usize {
        // Structurally zero: this file contains no verb that could commit.
        // The field is still read around every judgement so the receipt's value
        // comes from the backend rather than from a literal in the caller.
        0
    }
}
