use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_BORDER_COLOR};

pub fn suppress_accent_border(webview: &tauri::Webview) -> Result<(), String> {
    let hwnd = webview
        .window()
        .hwnd()
        .map_err(|error| format!("Tauri webview window HWND: {error}"))?;
    // COLORREF is 0x00BBGGRR. Match oslprivacy.com's #0a0a0a shell instead
    // of inheriting the user's Windows accent (which can look like an OSL
    // status or protection signal around the entire window).
    let colour: u32 = 0x000a_0a0a;
    let result = unsafe {
        DwmSetWindowAttribute(
            hwnd.0 as windows_sys::Win32::Foundation::HWND,
            DWMWA_BORDER_COLOR as u32,
            (&colour as *const u32).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    };
    if result < 0 {
        return Err("Windows rejected the neutral OSL border".to_owned());
    }
    Ok(())
}
