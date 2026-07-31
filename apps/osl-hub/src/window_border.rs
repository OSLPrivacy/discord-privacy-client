use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};

pub fn suppress_accent_border(webview: &tauri::Webview) -> Result<(), String> {
    let hwnd = webview
        .window()
        .hwnd()
        .map_err(|error| format!("Tauri webview window HWND: {error}"))?;
    // DWMWA_COLOR_NONE removes the system accent outline entirely. A themed
    // Windows border can otherwise look like an OSL protection/status signal
    // and remains visible even though the bundled UI has no border.
    const DWMWA_COLOR_NONE: u32 = 0xffff_fffe;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as windows_sys::Win32::Foundation::HWND,
            DWMWA_BORDER_COLOR as u32,
            (&DWMWA_COLOR_NONE as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if result < 0 {
        // DWMWA_BORDER_COLOR is Windows 11+. On Windows 10 a borderless,
        // resizable HWND can still receive the active accent outline. Disable
        // only DWM's non-client rendering there; the bundled titlebar and
        // client area remain unchanged and the resize style stays intact.
        const DWMWA_NCRENDERING_POLICY: u32 = 2;
        const DWMNCRP_DISABLED: u32 = 1;
        let fallback = unsafe {
            DwmSetWindowAttribute(
                hwnd.0 as windows_sys::Win32::Foundation::HWND,
                DWMWA_NCRENDERING_POLICY,
                (&DWMNCRP_DISABLED as *const u32).cast(),
                std::mem::size_of::<u32>() as u32,
            )
        };
        if fallback < 0 {
            return Err("Windows rejected the neutral OSL border".to_owned());
        }
    }
    Ok(())
}
