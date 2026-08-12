//! TASK 6860 — real OS capture against the shipped screenshot shield.
//!
//! Two real top-level windows are created and filled with the same known
//! colour. One is left alone; the other is handed to the production
//! `story_privacy::apply_shield_to_window`, which is the same call the story
//! surface makes. The desktop is then captured twice through the ordinary
//! Win32 screen-capture path (`BitBlt` from the screen DC with `SRCCOPY |
//! CAPTUREBLT` — what Snipping Tool, Game Bar and OBS use):
//!
//! * **baseline** — before the shield is applied, so both windows must show
//!   the known colour. Without this pass, a black rectangle would prove only
//!   that a window failed to paint.
//! * **shielded** — after the shield is applied, so the control must still
//!   show the known colour and the shielded window must not.
//!
//! `--cosmetic` is the red-proof mode: it skips the primitive entirely and
//! claims protection anyway, exactly as a decorative shield would. The capture
//! then shows the shielded window in full colour, which is how the check
//! catches it.
//!
//! Off Windows nothing is captured and nothing is claimed: the binary reports
//! the honest disabled state and exits 0, because "no primitive here" is a
//! result, not a failure.

use std::path::PathBuf;

use serde_json::json;

fn main() {
    let mut out = PathBuf::from("capture-6860.json");
    let mut cosmetic = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next().unwrap_or_default()),
            "--cosmetic" => cosmetic = true,
            other => {
                eprintln!("TASK6860 capture proof: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    let report = run(&out, cosmetic);
    std::fs::write(&out, serde_json::to_vec_pretty(&report).expect("report json"))
        .expect("write capture report");
    println!(
        "TASK6860 capture report written to {} (platform_supported={})",
        out.display(),
        report
            .get("platform_supported")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    );
}

#[cfg(not(windows))]
fn run(_out: &std::path::Path, cosmetic: bool) -> serde_json::Value {
    // The honest unsupported branch. The setting is disabled, the copy makes no
    // claim, and no capture is attempted because there is nothing to prove.
    let state = story_privacy::shield_state(true);
    let application = story_privacy::apply_shield_to_window(0, true);
    json!({
        "task": "6860",
        "target_os": std::env::consts::OS,
        "platform_supported": false,
        "cosmetic_mode": cosmetic,
        "capture_attempted": false,
        "primitive": state.primitive,
        "control_enabled": state.control_enabled,
        "setting_on": state.setting_on,
        "claims_protection": state.claims_protection || application.claims_protection,
        "disclosure": state.disclosure,
        "unavailable_copy": state.unavailable_copy,
        "application_error": application.error,
        "application_enforced": application.enforced,
    })
}

#[cfg(windows)]
fn run(out: &std::path::Path, cosmetic: bool) -> serde_json::Value {
    windows_impl::run(out, cosmetic)
}

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use std::path::Path;

    use serde_json::json;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DeleteDC,
        DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY, UpdateWindow,
    };
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowDisplayAffinity,
        GetWindowRect, PeekMessageW, RegisterClassExW, ShowWindow, TranslateMessage, HWND_TOPMOST, MSG, PM_REMOVE, SWP_NOACTIVATE, SW_SHOWNOACTIVATE, WNDCLASSEXW,
        WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    };

    /// 0x00BBGGRR — R=30, G=200, B=100. Nothing on a desktop is this colour by
    /// accident, and it is far from black in every channel.
    const KNOWN_COLOUR: u32 = 0x0064_C81E;
    const KNOWN_B: u8 = 0x64;
    const KNOWN_G: u8 = 0xC8;
    const KNOWN_R: u8 = 0x1E;
    const WINDOW_W: i32 = 300;
    const WINDOW_H: i32 = 200;
    /// Ignore an eight-pixel frame so a one-pixel border or a rounded corner
    /// cannot decide the verdict.
    const INSET: i32 = 8;

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn pump(rounds: u32) {
        for _ in 0..rounds {
            let mut message: MSG = unsafe { std::mem::zeroed() };
            while unsafe { PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0
            {
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }

    struct Capture {
        width: i32,
        height: i32,
        pixels: Vec<u8>,
    }

    impl Capture {
        /// One real desktop capture through the ordinary Win32 path.
        fn take(left: i32, top: i32, width: i32, height: i32) -> Option<Capture> {
            unsafe {
                let screen = GetDC(std::ptr::null_mut());
                if screen.is_null() {
                    return None;
                }
                let memory = CreateCompatibleDC(screen);
                let bitmap = CreateCompatibleBitmap(screen, width, height);
                if memory.is_null() || bitmap.is_null() {
                    if !memory.is_null() {
                        DeleteDC(memory);
                    }
                    ReleaseDC(std::ptr::null_mut(), screen);
                    return None;
                }
                let previous = SelectObject(memory, bitmap);
                let copied = BitBlt(
                    memory,
                    0,
                    0,
                    width,
                    height,
                    screen,
                    left,
                    top,
                    SRCCOPY | CAPTUREBLT,
                ) != 0;
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    // Negative height: top-down rows, so row 0 is the top of
                    // the screen region and the arithmetic below is obvious.
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: (width * height * 4) as u32,
                    biXPelsPerMeter: 0,
                    biYPelsPerMeter: 0,
                    biClrUsed: 0,
                    biClrImportant: 0,
                };
                let mut pixels = vec![0u8; (width * height * 4) as usize];
                let rows = if copied {
                    GetDIBits(
                        memory,
                        bitmap,
                        0,
                        height as u32,
                        pixels.as_mut_ptr().cast::<c_void>(),
                        &mut info,
                        DIB_RGB_COLORS,
                    )
                } else {
                    0
                };
                SelectObject(memory, previous);
                DeleteObject(bitmap);
                DeleteDC(memory);
                ReleaseDC(std::ptr::null_mut(), screen);
                if rows != height {
                    return None;
                }
                Some(Capture {
                    width,
                    height,
                    pixels,
                })
            }
        }

        fn region_stats(&self, origin_x: i32, origin_y: i32, rect: &RECT) -> RegionStats {
            let left = (rect.left - origin_x + INSET).max(0);
            let top = (rect.top - origin_y + INSET).max(0);
            let right = (rect.right - origin_x - INSET).min(self.width);
            let bottom = (rect.bottom - origin_y - INSET).min(self.height);
            let mut stats = RegionStats::default();
            let mut y = top;
            while y < bottom {
                let mut x = left;
                while x < right {
                    let index = ((y * self.width + x) * 4) as usize;
                    let blue = self.pixels[index];
                    let green = self.pixels[index + 1];
                    let red = self.pixels[index + 2];
                    stats.total += 1;
                    if blue == KNOWN_B && green == KNOWN_G && red == KNOWN_R {
                        stats.known_colour += 1;
                    }
                    if blue == 0 && green == 0 && red == 0 {
                        stats.black += 1;
                    }
                    x += 1;
                }
                y += 1;
            }
            stats
        }

        /// Write the capture out as a 32-bit BMP so the evidence is a real
        /// image on disk, not a claim about pixels.
        fn write_bmp(&self, path: &Path) -> std::io::Result<u64> {
            let row_bytes = (self.width * 4) as usize;
            let pixel_bytes = row_bytes * self.height as usize;
            let mut file = Vec::with_capacity(pixel_bytes + 54);
            file.extend_from_slice(b"BM");
            file.extend_from_slice(&((pixel_bytes + 54) as u32).to_le_bytes());
            file.extend_from_slice(&0u32.to_le_bytes());
            file.extend_from_slice(&54u32.to_le_bytes());
            file.extend_from_slice(&40u32.to_le_bytes());
            file.extend_from_slice(&self.width.to_le_bytes());
            file.extend_from_slice(&self.height.to_le_bytes());
            file.extend_from_slice(&1u16.to_le_bytes());
            file.extend_from_slice(&32u16.to_le_bytes());
            file.extend_from_slice(&0u32.to_le_bytes());
            file.extend_from_slice(&(pixel_bytes as u32).to_le_bytes());
            file.extend_from_slice(&2835i32.to_le_bytes());
            file.extend_from_slice(&2835i32.to_le_bytes());
            file.extend_from_slice(&0u32.to_le_bytes());
            file.extend_from_slice(&0u32.to_le_bytes());
            // BMP rows run bottom-up; the capture is top-down.
            for row in (0..self.height as usize).rev() {
                let start = row * row_bytes;
                file.extend_from_slice(&self.pixels[start..start + row_bytes]);
            }
            std::fs::write(path, &file)?;
            Ok(file.len() as u64)
        }
    }

    #[derive(Default)]
    struct RegionStats {
        total: u64,
        known_colour: u64,
        black: u64,
    }

    impl RegionStats {
        fn as_json(&self) -> serde_json::Value {
            json!({
                "total_pixels": self.total,
                "known_colour_pixels": self.known_colour,
                "black_pixels": self.black,
            })
        }
    }

    fn affinity(hwnd: HWND) -> u32 {
        let mut value = 0u32;
        unsafe {
            GetWindowDisplayAffinity(hwnd, &mut value);
        }
        value
    }

    fn window_rect(hwnd: HWND) -> RECT {
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        unsafe {
            GetWindowRect(hwnd, &mut rect);
        }
        rect
    }

    pub fn run(out: &Path, cosmetic: bool) -> serde_json::Value {
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class_name = wide("OslTask6860ShieldProof");
        let brush = unsafe { CreateSolidBrush(KNOWN_COLOUR) };
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: brush,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        let atom = unsafe { RegisterClassExW(&class) };
        if atom == 0 {
            return json!({
                "task": "6860",
                "target_os": "windows",
                "platform_supported": true,
                "capture_attempted": false,
                "error": "window class registration failed",
            });
        }

        let make = |x: i32, title: &str| -> HWND {
            let title = wide(title);
            unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                    class_name.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP | WS_VISIBLE,
                    x,
                    40,
                    WINDOW_W,
                    WINDOW_H,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    instance,
                    std::ptr::null(),
                )
            }
        };
        let control = make(40, "OSL 6860 control");
        let shielded = make(40 + WINDOW_W + 40, "OSL 6860 shielded story");
        if control.is_null() || shielded.is_null() {
            return json!({
                "task": "6860",
                "target_os": "windows",
                "platform_supported": true,
                "capture_attempted": false,
                "error": "window creation failed",
            });
        }
        unsafe {
            ShowWindow(control, SW_SHOWNOACTIVATE);
            ShowWindow(shielded, SW_SHOWNOACTIVATE);
            UpdateWindow(control);
            UpdateWindow(shielded);
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                control,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
            );
            windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
                shielded,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                    | windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
            );
        }
        pump(16);

        let control_rect = window_rect(control);
        let shielded_rect = window_rect(shielded);
        let origin_x = control_rect.left.min(shielded_rect.left) - 10;
        let origin_y = control_rect.top.min(shielded_rect.top) - 10;
        let width = control_rect.right.max(shielded_rect.right) - origin_x + 10;
        let height = control_rect.bottom.max(shielded_rect.bottom) - origin_y + 10;

        // Pass 1: nothing is protected yet.
        let baseline = Capture::take(origin_x, origin_y, width, height);
        let baseline_json = match &baseline {
            Some(capture) => {
                let path = out.with_extension("baseline.bmp");
                let bytes = capture.write_bmp(&path).unwrap_or(0);
                json!({
                    "captured": true,
                    "image": path.to_string_lossy(),
                    "image_bytes": bytes,
                    "control": capture.region_stats(origin_x, origin_y, &control_rect).as_json(),
                    "shielded": capture.region_stats(origin_x, origin_y, &shielded_rect).as_json(),
                })
            }
            None => json!({"captured": false}),
        };

        // Apply the shield through the production entry point — or, in the red
        // proof, do not apply it and claim protection anyway.
        let application = if cosmetic {
            json!({
                "requested_on": true,
                "primitive": serde_json::Value::Null,
                "enforced": false,
                "claims_protection": true,
                "disclosure": story_privacy::SHIELD_DISCLOSURE,
                "unavailable_copy": serde_json::Value::Null,
                "error": serde_json::Value::Null,
                "mode": "cosmetic-overlay-only",
            })
        } else {
            let applied = story_privacy::apply_shield_to_window(shielded as isize, true);
            let mut value = serde_json::to_value(&applied).expect("application json");
            if let Some(map) = value.as_object_mut() {
                map.insert("mode".into(), json!("production-primitive"));
            }
            value
        };
        pump(12);

        // Pass 2: the same capture, with the shield in force.
        let shielded_capture = Capture::take(origin_x, origin_y, width, height);
        let shielded_json = match &shielded_capture {
            Some(capture) => {
                let path = out.with_extension("shielded.bmp");
                let bytes = capture.write_bmp(&path).unwrap_or(0);
                json!({
                    "captured": true,
                    "image": path.to_string_lossy(),
                    "image_bytes": bytes,
                    "control": capture.region_stats(origin_x, origin_y, &control_rect).as_json(),
                    "shielded": capture.region_stats(origin_x, origin_y, &shielded_rect).as_json(),
                })
            }
            None => json!({"captured": false}),
        };

        let control_affinity = affinity(control);
        let shielded_affinity = affinity(shielded);
        unsafe {
            DestroyWindow(control);
            DestroyWindow(shielded);
            DeleteObject(brush);
        }
        pump(2);

        json!({
            "task": "6860",
            "target_os": "windows",
            "platform_supported": true,
            "cosmetic_mode": cosmetic,
            "capture_attempted": true,
            "capture_method": "win32-bitblt-srccopy-captureblt-desktop-dc",
            "primitive": story_privacy::SHIELD_PRIMITIVE,
            "known_colour_rgb": [KNOWN_R, KNOWN_G, KNOWN_B],
            "capture_region": {"left": origin_x, "top": origin_y, "width": width, "height": height},
            "control_rect": [control_rect.left, control_rect.top, control_rect.right, control_rect.bottom],
            "shielded_rect": [shielded_rect.left, shielded_rect.top, shielded_rect.right, shielded_rect.bottom],
            "affinity_control": control_affinity,
            "affinity_shielded": shielded_affinity,
            "expected_shielded_affinity": 0x11,
            "application": application,
            "baseline_capture": baseline_json,
            "shielded_capture": shielded_json,
            "disclosure": story_privacy::SHIELD_DISCLOSURE,
        })
    }
}
