//! OSL-owned image viewer for decrypted native-overlay attachments.
//!
//! Encoded and decoded pixels stay in this process. The window is created
//! hidden, capture exclusion is applied and read back, and only then may the
//! caller reveal it. Closing the window zeroizes the retained pixel buffer.

#[cfg(windows)]
mod windows_viewer {
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    };
    use windows::Win32::Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICImagingFactory,
        WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnLoad,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        BeginPaint, EndPaint, InvalidateRect, PatBlt, StretchDIBits, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, BLACKNESS, DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallWindowProcW, GetClientRect, GetWindowDisplayAffinity, GetWindowLongPtrW,
        SetWindowLongPtrW, GWLP_WNDPROC, WDA_EXCLUDEFROMCAPTURE, WM_ERASEBKGND, WM_NCDESTROY,
        WM_PAINT, WNDPROC,
    };
    use zeroize::{Zeroize, Zeroizing};

    const MAX_DECODED_PIXELS: u64 = 64 * 1024 * 1024;

    struct ViewerPixels {
        previous_proc: isize,
        width: u32,
        height: u32,
        pixels: Zeroizing<Vec<u8>>,
    }

    fn viewers() -> &'static Mutex<HashMap<isize, ViewerPixels>> {
        static VIEWERS: OnceLock<Mutex<HashMap<isize, ViewerPixels>>> = OnceLock::new();
        VIEWERS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    static NEXT_LABEL: AtomicU64 = AtomicU64::new(0);

    pub(crate) struct PreparedImageViewer {
        window: Option<tauri::Window>,
    }

    impl PreparedImageViewer {
        pub(crate) fn show(mut self) -> Result<(), String> {
            let window = self
                .window
                .take()
                .ok_or_else(|| "The protected image viewer is unavailable".to_owned())?;
            window
                .show()
                .map_err(|_| "The protected image viewer could not be shown".to_owned())?;
            let hwnd = window
                .hwnd()
                .map_err(|_| "The protected image viewer is unavailable".to_owned())?
                .0 as HWND;
            unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
            let _ = window.set_focus();
            Ok(())
        }
    }

    impl Drop for PreparedImageViewer {
        fn drop(&mut self) {
            if let Some(window) = self.window.take() {
                let _ = window.close();
            }
        }
    }

    struct ComApartment;

    impl ComApartment {
        fn initialize() -> Result<Self, String> {
            unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
                .ok()
                .map_err(|_| "The protected image decoder could not start".to_owned())?;
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn decode_image(
        mut encoded: Zeroizing<Vec<u8>>,
    ) -> Result<(u32, u32, Zeroizing<Vec<u8>>), String> {
        let _com = ComApartment::initialize()?;
        let result = (|| unsafe {
            let factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                    .map_err(|_| "The protected image decoder is unavailable".to_owned())?;
            let stream = factory
                .CreateStream()
                .map_err(|_| "The protected image decoder is unavailable".to_owned())?;
            stream
                .InitializeFromMemory(encoded.as_slice())
                .map_err(|_| "The protected image is invalid".to_owned())?;
            let decoder = factory
                .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnLoad)
                .map_err(|_| "The protected image is invalid or unsupported".to_owned())?;
            if decoder.GetFrameCount().unwrap_or(0) != 1 {
                return Err("Animated protected images are not supported".to_owned());
            }
            let frame = decoder
                .GetFrame(0)
                .map_err(|_| "The protected image is invalid".to_owned())?;
            let mut width = 0u32;
            let mut height = 0u32;
            frame
                .GetSize(&mut width, &mut height)
                .map_err(|_| "The protected image dimensions are invalid".to_owned())?;
            let pixel_count = u64::from(width)
                .checked_mul(u64::from(height))
                .filter(|count| *count > 0 && *count <= MAX_DECODED_PIXELS)
                .ok_or_else(|| {
                    "The protected image dimensions exceed the safe viewer limit".to_owned()
                })?;
            let byte_len = pixel_count
                .checked_mul(4)
                .and_then(|length| usize::try_from(length).ok())
                .ok_or_else(|| "The protected image dimensions are invalid".to_owned())?;
            let stride = width
                .checked_mul(4)
                .ok_or_else(|| "The protected image dimensions are invalid".to_owned())?;
            let converter = factory
                .CreateFormatConverter()
                .map_err(|_| "The protected image decoder is unavailable".to_owned())?;
            converter
                .Initialize(
                    &frame,
                    &GUID_WICPixelFormat32bppBGRA,
                    WICBitmapDitherTypeNone,
                    None,
                    0.0,
                    WICBitmapPaletteTypeCustom,
                )
                .map_err(|_| "The protected image pixel format is unsupported".to_owned())?;
            let mut pixels = Zeroizing::new(vec![0u8; byte_len]);
            converter
                .CopyPixels(std::ptr::null(), stride, pixels.as_mut_slice())
                .map_err(|_| "The protected image could not be decoded".to_owned())?;
            Ok((width, height, pixels))
        })();
        encoded.zeroize();
        result
    }

    unsafe extern "system" fn viewer_window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if message == WM_PAINT {
            if let Ok(guard) = viewers().lock() {
                if let Some(viewer) = guard.get(&(hwnd as isize)) {
                    let mut paint: PAINTSTRUCT = std::mem::zeroed();
                    let hdc = BeginPaint(hwnd, &mut paint);
                    let mut client: RECT = std::mem::zeroed();
                    if !hdc.is_null() && GetClientRect(hwnd, &mut client) != 0 {
                        let client_width = (client.right - client.left).max(1);
                        let client_height = (client.bottom - client.top).max(1);
                        PatBlt(hdc, 0, 0, client_width, client_height, BLACKNESS);
                        let scale = (client_width as f64 / f64::from(viewer.width))
                            .min(client_height as f64 / f64::from(viewer.height));
                        let draw_width = (f64::from(viewer.width) * scale).round() as i32;
                        let draw_height = (f64::from(viewer.height) * scale).round() as i32;
                        let x = (client_width - draw_width) / 2;
                        let y = (client_height - draw_height) / 2;
                        let mut bitmap: BITMAPINFO = std::mem::zeroed();
                        bitmap.bmiHeader = BITMAPINFOHEADER {
                            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                            biWidth: viewer.width as i32,
                            biHeight: -(viewer.height as i32),
                            biPlanes: 1,
                            biBitCount: 32,
                            biCompression: BI_RGB,
                            biSizeImage: viewer.pixels.len() as u32,
                            biXPelsPerMeter: 0,
                            biYPelsPerMeter: 0,
                            biClrUsed: 0,
                            biClrImportant: 0,
                        };
                        StretchDIBits(
                            hdc,
                            x,
                            y,
                            draw_width,
                            draw_height,
                            0,
                            0,
                            viewer.width as i32,
                            viewer.height as i32,
                            viewer.pixels.as_ptr().cast(),
                            &bitmap,
                            DIB_RGB_COLORS,
                            SRCCOPY,
                        );
                    }
                    EndPaint(hwnd, &paint);
                    return 0;
                }
            }
        }
        if message == WM_ERASEBKGND {
            return 1;
        }
        let previous = viewers()
            .lock()
            .ok()
            .and_then(|mut guard| {
                if message == WM_NCDESTROY {
                    guard
                        .remove(&(hwnd as isize))
                        .map(|viewer| viewer.previous_proc)
                } else {
                    guard
                        .get(&(hwnd as isize))
                        .map(|viewer| viewer.previous_proc)
                }
            })
            .unwrap_or(0);
        if previous == 0 {
            return 0;
        }
        let procedure: WNDPROC = Some(std::mem::transmute(previous));
        CallWindowProcW(procedure, hwnd, message, wparam, lparam)
    }

    pub(crate) fn prepare(
        app: &tauri::AppHandle,
        encoded: Zeroizing<Vec<u8>>,
    ) -> Result<PreparedImageViewer, String> {
        let (width, height, pixels) = decode_image(encoded)?;
        let max_width = width.min(1200).max(320);
        let max_height = height.min(800).max(240);
        let label = format!(
            "native-image-viewer-{}",
            NEXT_LABEL.fetch_add(1, Ordering::AcqRel).wrapping_add(1)
        );
        let window = tauri::window::WindowBuilder::new(app, label)
            .title("OSL private image")
            .inner_size(f64::from(max_width), f64::from(max_height))
            .min_inner_size(320.0, 240.0)
            .visible(false)
            .build()
            .map_err(|_| "The protected image viewer could not be created".to_owned())?;
        let hwnd = window
            .hwnd()
            .map_err(|_| "The protected image viewer is unavailable".to_owned())?
            .0 as HWND;
        if hwnd.is_null() {
            let _ = window.close();
            return Err("The protected image viewer is unavailable".to_owned());
        }
        let previous = unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) };
        if previous == 0 {
            let _ = window.close();
            return Err("The protected image viewer could not be secured".to_owned());
        }
        viewers()
            .lock()
            .map_err(|_| "The protected image viewer is unavailable".to_owned())?
            .insert(
                hwnd as isize,
                ViewerPixels {
                    previous_proc: previous,
                    width,
                    height,
                    pixels,
                },
            );
        if unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, viewer_window_proc as *const () as isize)
        } != previous
        {
            viewers()
                .lock()
                .ok()
                .and_then(|mut guard| guard.remove(&(hwnd as isize)));
            let _ = window.close();
            return Err("The protected image viewer could not be secured".to_owned());
        }
        if runtime::apply_to_hwnd_and_children(hwnd as isize, runtime::ScreenshotProtection::On)
            .is_err()
        {
            let _ = window.close();
            return Err(
                "The protected image viewer could not enable capture resistance".to_owned(),
            );
        }
        let mut affinity = 0u32;
        if unsafe { GetWindowDisplayAffinity(hwnd, &mut affinity) } == 0
            || affinity != WDA_EXCLUDEFROMCAPTURE
        {
            let _ = window.close();
            return Err(
                "The protected image viewer could not verify capture resistance".to_owned(),
            );
        }
        Ok(PreparedImageViewer {
            window: Some(window),
        })
    }
}

#[cfg(windows)]
pub(crate) use windows_viewer::prepare;

#[cfg(not(windows))]
pub(crate) struct PreparedImageViewer;

#[cfg(not(windows))]
impl PreparedImageViewer {
    pub(crate) fn show(self) -> Result<(), String> {
        Err("The protected image viewer requires Windows".to_owned())
    }
}

#[cfg(not(windows))]
pub(crate) fn prepare(
    _app: &tauri::AppHandle,
    _encoded: zeroize::Zeroizing<Vec<u8>>,
) -> Result<PreparedImageViewer, String> {
    Err("The protected image viewer requires Windows".to_owned())
}
