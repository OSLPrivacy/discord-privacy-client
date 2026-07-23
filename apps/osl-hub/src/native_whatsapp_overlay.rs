//! Capture-resistant OSL-owned composer placed only over a verified WhatsApp composer.
//!
//! Geometry is never authority. The caller must first obtain a complete
//! `WhatsAppVerificationReceipt`; this module accepts no account, chat,
//! recipient, message, path, URL, or credential input.

use std::path::PathBuf;
use tauri::{webview::NewWindowResponse, Manager, WebviewUrl};

pub(crate) const OVERLAY_LABEL: &str = "composer-overlay";
const OVERLAY_ASSET: &str = "overlay.html";

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct OverlayRect {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn verified_overlay_rect(window: [i32; 4], composer: [i32; 4]) -> Option<OverlayRect> {
    let window_width = window[2].checked_sub(window[0])?;
    let window_height = window[3].checked_sub(window[1])?;
    let width = composer[2].checked_sub(composer[0])?;
    let height = composer[3].checked_sub(composer[1])?;
    if window_width < 480
        || window_height < 360
        || width < 240
        || !(36..=220).contains(&height)
        || composer[0] < window[0]
        || composer[1] < window[1] + window_height / 2
        || composer[2] > window[2]
        || composer[3] > window[3]
    {
        return None;
    }
    Some(OverlayRect {
        x: composer[0],
        y: composer[1],
        width: width.try_into().ok()?,
        height: height.try_into().ok()?,
    })
}

fn bundled_navigation(url: &url::Url) -> bool {
    let local = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http"
            && url.host_str() == Some("tauri.localhost")
            && url.port().is_none());
    local
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(url.path(), "/overlay.html" | "/overlay.html/")
}

pub(crate) fn hide(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(OVERLAY_LABEL) {
        let _ = window.hide();
        let _ = window.close();
    }
}

pub(crate) fn show_verified(
    app: &tauri::AppHandle,
    window_rect: [i32; 4],
    composer_rect: [i32; 4],
) -> Result<(), String> {
    let rect = verified_overlay_rect(window_rect, composer_rect)
        .ok_or_else(|| "The verified WhatsApp composer geometry was rejected".to_owned())?;
    hide(app);
    let window = tauri::WebviewWindowBuilder::new(
        app,
        OVERLAY_LABEL,
        WebviewUrl::App(PathBuf::from(OVERLAY_ASSET)),
    )
    .title("OSL protected WhatsApp composer")
    .position(f64::from(rect.x), f64::from(rect.y))
    .inner_size(f64::from(rect.width), f64::from(rect.height))
    .transparent(false)
    .decorations(false)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(false)
    .focused(false)
    .visible(false)
    .devtools(false)
    .on_navigation(bundled_navigation)
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .build()
    .map_err(|_| "The protected WhatsApp composer could not be created safely".to_owned())?;
    if super::screenshot::apply_to_window(&window, runtime::ScreenshotProtection::On).is_err() {
        let _ = window.close();
        return Err(
            "The protected WhatsApp composer could not enable capture resistance".to_owned(),
        );
    }
    window
        .show()
        .map_err(|_| "The protected WhatsApp composer could not be shown safely".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_a_bounded_composer_inside_the_bottom_half() {
        let window = [100, 100, 1300, 900];
        assert_eq!(
            verified_overlay_rect(window, [450, 790, 1250, 860]),
            Some(OverlayRect {
                x: 450,
                y: 790,
                width: 800,
                height: 70
            })
        );
        assert!(verified_overlay_rect(window, [450, 300, 1250, 370]).is_none());
        assert!(verified_overlay_rect(window, [50, 790, 1250, 860]).is_none());
        assert!(verified_overlay_rect(window, [450, 790, 600, 860]).is_none());
    }
}
