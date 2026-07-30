//! Provider-neutral, bounded native-surface capture for adaptive protected UI.
//! Callers must supply already-verified screen-space outer and input rectangles.
//! Pixels remain in memory and are never written to disk, logged, or receipted.

use base64::Engine as _;
use osl_privacy_hub::native_discord_adapter::VerifiedComposerTextPresentation;
use serde::Serialize;
use std::sync::Mutex;

const MAX_SURFACE_WIDTH: i32 = 8_192;
const MAX_SURFACE_HEIGHT: i32 = 1_024;
const MAX_SURFACE_BYTES: usize = 8_192 * 1_024 * 4;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSurfaceCapture {
    version: &'static str,
    image_data_url: String,
    width_px: u32,
    height_px: u32,
    input_left_px: u32,
    input_top_px: u32,
    input_width_px: u32,
    input_height_px: u32,
    input_background: String,
    text_left_px: u32,
    text_top_px: u32,
    text_width_px: u32,
    text_height_px: u32,
    font_family: Option<String>,
    font_size_px: Option<f64>,
    font_weight: Option<u16>,
    line_height_px: Option<f64>,
    #[serde(skip)]
    presentation_insets: SurfaceInsets,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SurfaceInsets {
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
}

impl NativeSurfaceCapture {
    pub(crate) fn presentation_bounds(&self, outer: [i32; 4]) -> Option<[i32; 4]> {
        let left = outer[0].checked_add(i32::try_from(self.presentation_insets.left).ok()?)?;
        let top = outer[1].checked_add(i32::try_from(self.presentation_insets.top).ok()?)?;
        let right = outer[2].checked_sub(i32::try_from(self.presentation_insets.right).ok()?)?;
        let bottom = outer[3].checked_sub(i32::try_from(self.presentation_insets.bottom).ok()?)?;
        (right > left && bottom > top).then_some([left, top, right, bottom])
    }
}

/// Provider-neutral in-memory visual state. Provider adapters may publish only
/// pixels and verified geometry; renderer code never receives provider
/// selectors, native handles, account identifiers, or message contents.
#[derive(Default)]
pub(crate) struct NativeSurfaceCaptureState {
    current: Mutex<Option<(NativeSurfaceKey, NativeSurfaceCapture)>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeSurfaceKey {
    pub(crate) session_epoch: u64,
    pub(crate) host_generation: u64,
}

#[cfg(feature = "discord-qa-shell")]
fn qa_record_native_surface_matrix(
    width_px: u32,
    height_px: u32,
    input_width_px: u32,
    input_height_px: u32,
    typography_present: bool,
) {
    use std::io::Write as _;

    let path = std::env::temp_dir().join("osl-discord-qa-native-surface.txt");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        // Content-free QA evidence only: dimensions and booleans, never pixels,
        // text, handles, account identifiers, or the sampled colour value.
        let _ = writeln!(
            file,
            "surface_capture width_px={width_px} height_px={height_px} \
             input_width_px={input_width_px} input_height_px={input_height_px} \
             theme_sample=present nitro_sample=present typography={}",
            if typography_present {
                "present"
            } else {
                "absent"
            }
        );
    }
}

#[cfg(not(feature = "discord-qa-shell"))]
fn qa_record_native_surface_matrix(
    _width_px: u32,
    _height_px: u32,
    _input_width_px: u32,
    _input_height_px: u32,
    _typography_present: bool,
) {
}

impl NativeSurfaceCaptureState {
    pub(crate) fn replace(
        &self,
        key: NativeSurfaceKey,
        capture: NativeSurfaceCapture,
    ) -> Result<(), String> {
        *self
            .current
            .lock()
            .map_err(|_| "The adaptive native surface state is unavailable".to_owned())? =
            Some((key, capture));
        Ok(())
    }

    pub(crate) fn current(&self, key: NativeSurfaceKey) -> Option<NativeSurfaceCapture> {
        self.current.lock().ok().and_then(|current| {
            current
                .as_ref()
                .filter(|(stored, _)| *stored == key)
                .map(|(_, capture)| capture.clone())
        })
    }

    pub(crate) fn clear(&self) {
        if let Ok(mut current) = self.current.lock() {
            *current = None;
        }
    }
}

fn bounded_relative_input(
    outer: [i32; 4],
    input: [i32; 4],
) -> Option<(i32, i32, i32, i32, i32, i32)> {
    let width = outer[2].checked_sub(outer[0])?;
    let height = outer[3].checked_sub(outer[1])?;
    let input_left = input[0].checked_sub(outer[0])?;
    let input_top = input[1].checked_sub(outer[1])?;
    let input_width = input[2].checked_sub(input[0])?;
    let input_height = input[3].checked_sub(input[1])?;
    (width >= 160
        && width <= MAX_SURFACE_WIDTH
        && height >= 24
        && height <= MAX_SURFACE_HEIGHT
        && input_left >= 0
        && input_top >= 0
        && input_width > 0
        && input_height > 0
        && input_left.checked_add(input_width)? <= width
        && input_top.checked_add(input_height)? <= height)
        .then_some((
            width,
            height,
            input_left,
            input_top,
            input_width,
            input_height,
        ))
}

fn pixel_offset(width: usize, height: usize, x: usize, y_from_top: usize) -> Option<usize> {
    (x < width && y_from_top < height)
        .then(|| height - 1 - y_from_top)?
        .checked_mul(width)?
        .checked_add(x)?
        .checked_mul(4)
}

fn dominant_input_background(
    pixels: &[u8],
    width: usize,
    height: usize,
    input: [usize; 4],
) -> Option<[u8; 3]> {
    use std::collections::HashMap;

    let mut bins = HashMap::<u16, (u32, u32, u32, u32)>::new();
    for y in input[1]..input[3] {
        for x in input[0]..input[2] {
            let offset = pixel_offset(width, height, x, y)?;
            let blue = *pixels.get(offset)?;
            let green = *pixels.get(offset + 1)?;
            let red = *pixels.get(offset + 2)?;
            let key =
                (u16::from(red >> 3) << 10) | (u16::from(green >> 3) << 5) | u16::from(blue >> 3);
            let entry = bins.entry(key).or_default();
            entry.0 += u32::from(red);
            entry.1 += u32::from(green);
            entry.2 += u32::from(blue);
            entry.3 += 1;
        }
    }
    bins.into_values()
        .max_by_key(|entry| entry.3)
        .and_then(|(red, green, blue, count)| {
            (count > 0).then_some([
                u8::try_from(red / count).ok()?,
                u8::try_from(green / count).ok()?,
                u8::try_from(blue / count).ok()?,
            ])
        })
}

fn pixel_matches_background(pixel: &[u8], background: [u8; 3]) -> bool {
    let [red, green, blue] = background;
    pixel.len() >= 3
        && u16::from(pixel[2].abs_diff(red))
            + u16::from(pixel[1].abs_diff(green))
            + u16::from(pixel[0].abs_diff(blue))
            <= 12
}

fn band_matches(
    pixels: &[u8],
    width: usize,
    height: usize,
    background: [u8; 3],
    points: impl Iterator<Item = (usize, usize)>,
) -> bool {
    let mut matched = 0usize;
    let mut total = 0usize;
    for (x, y) in points {
        let Some(offset) = pixel_offset(width, height, x, y) else {
            return false;
        };
        total += 1;
        if pixel_matches_background(&pixels[offset..], background) {
            matched += 1;
        }
    }
    total >= 4 && matched.saturating_mul(100) >= total.saturating_mul(55)
}

fn scan_edge(
    start: usize,
    limit: usize,
    descending: bool,
    mut is_inside: impl FnMut(usize) -> bool,
) -> Option<usize> {
    let mut cursor = start;
    let mut last_inside = start;
    let mut misses = 0usize;
    loop {
        if is_inside(cursor) {
            last_inside = cursor;
            misses = 0;
        } else {
            misses += 1;
            if misses >= 2 {
                return Some(last_inside);
            }
        }
        if cursor == limit {
            return None;
        }
        cursor = if descending {
            cursor.checked_sub(1)?
        } else {
            cursor.checked_add(1)?
        };
    }
}

fn adaptive_surface_insets(
    pixels: &[u8],
    width: usize,
    height: usize,
    input: [usize; 4],
) -> SurfaceInsets {
    if width < 160
        || height < 24
        || input[0] >= input[2]
        || input[1] >= input[3]
        || input[2] > width
        || input[3] > height
    {
        return SurfaceInsets::default();
    }
    let Some(background) = dominant_input_background(pixels, width, height, input) else {
        return SurfaceInsets::default();
    };
    let vertical = input[1]..input[3];
    let horizontal = input[0]..input[2];
    let left_fill = scan_edge(input[0], 0, true, |x| {
        band_matches(
            pixels,
            width,
            height,
            background,
            vertical.clone().map(|y| (x, y)),
        )
    });
    let right_fill = scan_edge(input[2] - 1, width - 1, false, |x| {
        band_matches(
            pixels,
            width,
            height,
            background,
            vertical.clone().map(|y| (x, y)),
        )
    });
    let top_fill = scan_edge(input[1], 0, true, |y| {
        band_matches(
            pixels,
            width,
            height,
            background,
            horizontal.clone().map(|x| (x, y)),
        )
    });
    let bottom_fill = scan_edge(input[3] - 1, height - 1, false, |y| {
        band_matches(
            pixels,
            width,
            height,
            background,
            horizontal.clone().map(|x| (x, y)),
        )
    });

    // Include the composer's one-pixel native edge outside the detected fill.
    let left = left_fill.map_or(0, |edge| edge.saturating_sub(1));
    let top = top_fill.map_or(0, |edge| edge.saturating_sub(1));
    let right_edge = right_fill.map_or(width, |edge| edge.saturating_add(2).min(width));
    let bottom_edge = bottom_fill.map_or(height, |edge| edge.saturating_add(2).min(height));
    let insets = SurfaceInsets {
        left: u32::try_from(left).unwrap_or(0),
        top: u32::try_from(top).unwrap_or(0),
        right: u32::try_from(width.saturating_sub(right_edge)).unwrap_or(0),
        bottom: u32::try_from(height.saturating_sub(bottom_edge)).unwrap_or(0),
    };
    let trimmed_width = width.saturating_sub(left + usize::try_from(insets.right).unwrap_or(width));
    let trimmed_height =
        height.saturating_sub(top + usize::try_from(insets.bottom).unwrap_or(height));
    let input_stays_inside =
        left <= input[0] && top <= input[1] && right_edge >= input[2] && bottom_edge >= input[3];
    if input_stays_inside
        && trimmed_width >= 160
        && trimmed_height >= 24
        && left <= 112
        && usize::try_from(insets.right).is_ok_and(|value| value <= 240)
        && top <= 48
        && usize::try_from(insets.bottom).is_ok_and(|value| value <= 48)
    {
        insets
    } else {
        SurfaceInsets::default()
    }
}

fn crop_bottom_up_bgra(
    pixels: &[u8],
    width: usize,
    height: usize,
    insets: SurfaceInsets,
) -> Option<(Vec<u8>, usize, usize)> {
    let left = usize::try_from(insets.left).ok()?;
    let top = usize::try_from(insets.top).ok()?;
    let right = width.checked_sub(usize::try_from(insets.right).ok()?)?;
    let bottom = height.checked_sub(usize::try_from(insets.bottom).ok()?)?;
    let cropped_width = right.checked_sub(left)?;
    let cropped_height = bottom.checked_sub(top)?;
    let mut cropped =
        Vec::with_capacity(cropped_width.checked_mul(cropped_height)?.checked_mul(4)?);
    for y_from_bottom in (top..bottom).rev() {
        let row_from_top = y_from_bottom;
        let start = pixel_offset(width, height, left, row_from_top)?;
        let bytes = cropped_width.checked_mul(4)?;
        cropped.extend_from_slice(pixels.get(start..start.checked_add(bytes)?)?);
    }
    Some((cropped, cropped_width, cropped_height))
}

#[cfg(target_os = "windows")]
pub(crate) fn capture_verified_surface(
    outer: [i32; 4],
    input: [i32; 4],
    text_presentation: Option<VerifiedComposerTextPresentation>,
) -> Result<NativeSurfaceCapture, String> {
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };

    let (width, height, input_left, input_top, input_width, input_height) =
        bounded_relative_input(outer, input)
            .ok_or_else(|| "The verified native surface geometry is invalid".to_owned())?;
    let stride = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or_else(|| "The verified native surface is too large".to_owned())?;
    let pixel_bytes = stride
        .checked_mul(usize::try_from(height).unwrap_or(usize::MAX))
        .filter(|value| *value <= MAX_SURFACE_BYTES)
        .ok_or_else(|| "The verified native surface is too large".to_owned())?;

    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    if screen.is_null() {
        return Err("The verified native surface could not be captured".to_owned());
    }
    let memory = unsafe { CreateCompatibleDC(screen) };
    let bitmap = if memory.is_null() {
        std::ptr::null_mut()
    } else {
        unsafe { CreateCompatibleBitmap(screen, width, height) }
    };
    if memory.is_null() || bitmap.is_null() {
        if !memory.is_null() {
            unsafe { DeleteDC(memory) };
        }
        unsafe { ReleaseDC(std::ptr::null_mut(), screen) };
        return Err("The verified native surface could not be captured".to_owned());
    }
    let previous = unsafe { SelectObject(memory, bitmap) };
    let copied = unsafe {
        BitBlt(
            memory, 0, 0, width, height, screen, outer[0], outer[1], SRCCOPY,
        )
    } != 0;
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: u32::try_from(pixel_bytes).unwrap_or(0),
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let mut pixels = vec![0u8; pixel_bytes];
    let rows = if copied {
        unsafe {
            GetDIBits(
                memory,
                bitmap,
                0,
                height as u32,
                pixels.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            )
        }
    } else {
        0
    };
    unsafe {
        SelectObject(memory, previous);
        DeleteObject(bitmap);
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    if rows != height {
        return Err("The verified native surface could not be captured".to_owned());
    }

    let original_width = usize::try_from(width).unwrap_or(0);
    let original_height = usize::try_from(height).unwrap_or(0);
    let original_input = [
        usize::try_from(input_left).unwrap_or(0),
        usize::try_from(input_top).unwrap_or(0),
        usize::try_from(input_left + input_width).unwrap_or(0),
        usize::try_from(input_top + input_height).unwrap_or(0),
    ];
    let background_rgb =
        dominant_input_background(&pixels, original_width, original_height, original_input)
            .unwrap_or([pixels[2], pixels[1], pixels[0]]);
    let presentation_insets =
        adaptive_surface_insets(&pixels, original_width, original_height, original_input);
    let (pixels, captured_width, captured_height) = crop_bottom_up_bgra(
        &pixels,
        original_width,
        original_height,
        presentation_insets,
    )
    .ok_or_else(|| "The verified native surface crop is invalid".to_owned())?;
    let input_left = input_left
        .checked_sub(i32::try_from(presentation_insets.left).unwrap_or(0))
        .ok_or_else(|| "The verified native surface crop is invalid".to_owned())?;
    let input_top = input_top
        .checked_sub(i32::try_from(presentation_insets.top).unwrap_or(0))
        .ok_or_else(|| "The verified native surface crop is invalid".to_owned())?;
    let background = format!(
        "#{:02x}{:02x}{:02x}",
        background_rgb[0], background_rgb[1], background_rgb[2]
    );
    let text = text_presentation
        .filter(|presentation| {
            presentation.bounds.left >= input[0]
                && presentation.bounds.top >= input[1]
                && presentation.bounds.right <= input[2]
                && presentation.bounds.bottom <= input[3]
                && presentation.bounds.right > presentation.bounds.left
                && presentation.bounds.bottom > presentation.bounds.top
        })
        .map(|presentation| {
            let bounds = presentation.bounds;
            (
                bounds.left - outer[0],
                bounds.top - outer[1],
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                presentation,
            )
        });
    let (text_left, text_top, text_width, text_height, typography) = text
        .map(|(left, top, width, height, presentation)| {
            (left, top, width, height, Some(presentation))
        })
        .unwrap_or((
            input_left + i32::try_from(presentation_insets.left).unwrap_or(0),
            input_top + i32::try_from(presentation_insets.top).unwrap_or(0),
            input_width,
            input_height,
            None,
        ));
    let text_left = text_left
        .checked_sub(i32::try_from(presentation_insets.left).unwrap_or(0))
        .ok_or_else(|| "The verified native text crop is invalid".to_owned())?;
    let text_top = text_top
        .checked_sub(i32::try_from(presentation_insets.top).unwrap_or(0))
        .ok_or_else(|| "The verified native text crop is invalid".to_owned())?;
    let typography_values = typography.and_then(|presentation| {
        Some((
            presentation.font_family?,
            f64::from(presentation.font_size_milli_points?) / 1_000.0 * (96.0 / 72.0),
            presentation.font_weight?,
            f64::from(presentation.line_height_milli_px?) / 1_000.0,
        ))
    });
    qa_record_native_surface_matrix(
        u32::try_from(captured_width).unwrap_or(0),
        u32::try_from(captured_height).unwrap_or(0),
        input_width as u32,
        input_height as u32,
        typography_values.is_some(),
    );

    let file_size = 14usize
        .checked_add(40)
        .and_then(|value| value.checked_add(pixels.len()))
        .ok_or_else(|| "The verified native surface is too large".to_owned())?;
    let mut bmp = Vec::with_capacity(file_size);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&[0u8; 4]);
    bmp.extend_from_slice(&(54u32).to_le_bytes());
    bmp.extend_from_slice(&(40u32).to_le_bytes());
    bmp.extend_from_slice(&i32::try_from(captured_width).unwrap_or(0).to_le_bytes());
    bmp.extend_from_slice(&i32::try_from(captured_height).unwrap_or(0).to_le_bytes());
    bmp.extend_from_slice(&(1u16).to_le_bytes());
    bmp.extend_from_slice(&(32u16).to_le_bytes());
    bmp.extend_from_slice(&(0u32).to_le_bytes());
    bmp.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&[0u8; 16]);
    bmp.extend_from_slice(&pixels);

    Ok(NativeSurfaceCapture {
        version: "osl-native-surface-capture-v1",
        image_data_url: format!(
            "data:image/bmp;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bmp)
        ),
        width_px: u32::try_from(captured_width).unwrap_or(0),
        height_px: u32::try_from(captured_height).unwrap_or(0),
        input_left_px: input_left as u32,
        input_top_px: input_top as u32,
        input_width_px: input_width as u32,
        input_height_px: input_height as u32,
        input_background: background,
        text_left_px: text_left as u32,
        text_top_px: text_top as u32,
        text_width_px: text_width as u32,
        text_height_px: text_height as u32,
        font_family: typography_values.as_ref().map(|value| value.0.clone()),
        font_size_px: typography_values.as_ref().map(|value| value.1),
        font_weight: typography_values.as_ref().map(|value| value.2),
        line_height_px: typography_values.as_ref().map(|value| value.3),
        presentation_insets,
    })
}

/// Capture only while the provider adapter independently proves that the same
/// exact native target still owns the supplied screen-space geometry.
pub(crate) fn capture_verified_surface_guarded(
    outer: [i32; 4],
    input: [i32; 4],
    text_presentation: Option<VerifiedComposerTextPresentation>,
    mut target_is_current: impl FnMut() -> bool,
) -> Result<NativeSurfaceCapture, String> {
    if !target_is_current() {
        return Err("The verified native surface changed before capture".to_owned());
    }
    let capture = capture_verified_surface(outer, input, text_presentation)?;
    if !target_is_current() {
        return Err("The verified native surface changed during capture".to_owned());
    }
    Ok(capture)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn capture_verified_surface(
    _outer: [i32; 4],
    _input: [i32; 4],
    _text_presentation: Option<VerifiedComposerTextPresentation>,
) -> Result<NativeSurfaceCapture, String> {
    Err("Native surface capture requires Windows".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_surface(
        width: usize,
        height: usize,
        visible: [usize; 4],
        input: [usize; 4],
    ) -> Vec<u8> {
        let mut pixels = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let offset = pixel_offset(width, height, x, y).unwrap();
                let inside = x >= visible[0] && x < visible[2] && y >= visible[1] && y < visible[3];
                let edge = inside
                    && (x == visible[0]
                        || x + 1 == visible[2]
                        || y == visible[1]
                        || y + 1 == visible[3]);
                let color = if edge {
                    [40, 42, 47]
                } else if inside {
                    [55, 57, 63]
                } else {
                    [49, 51, 56]
                };
                pixels[offset] = color[2];
                pixels[offset + 1] = color[1];
                pixels[offset + 2] = color[0];
                pixels[offset + 3] = 255;
            }
        }
        // Representative placeholder/icon noise must not alter the dominant
        // fill or the majority band scans.
        for x in (input[0] + 8)..(input[0] + 32).min(input[2]) {
            let y = input[1] + (input[3] - input[1]) / 2;
            let offset = pixel_offset(width, height, x, y).unwrap();
            pixels[offset..offset + 3].copy_from_slice(&[220, 220, 220]);
        }
        pixels
    }

    #[test]
    fn bounds_require_input_to_stay_inside_bounded_surface() {
        assert_eq!(
            bounded_relative_input([100, 200, 700, 280], [150, 220, 600, 260]),
            Some((600, 80, 50, 20, 450, 40))
        );
        assert!(bounded_relative_input([100, 200, 700, 280], [90, 220, 600, 260]).is_none());
        assert!(bounded_relative_input([0, 0, 9000, 80], [10, 10, 8000, 60]).is_none());
    }

    #[test]
    fn pixel_trim_removes_footer_padding_without_cropping_editable_bounds() {
        let width = 752;
        let height = 66;
        let visible = [8, 0, 744, 58];
        let input = [64, 12, 620, 48];
        let pixels = synthetic_surface(width, height, visible, input);
        let insets = adaptive_surface_insets(&pixels, width, height, input);
        assert_eq!(
            insets,
            SurfaceInsets {
                left: 8,
                top: 0,
                right: 8,
                bottom: 8,
            }
        );
        let (cropped, cropped_width, cropped_height) =
            crop_bottom_up_bgra(&pixels, width, height, insets).expect("crop");
        assert_eq!((cropped_width, cropped_height), (736, 58));
        assert_eq!(cropped.len(), 736 * 58 * 4);
        assert!(usize::try_from(insets.left).unwrap() <= input[0]);
        assert!(usize::try_from(insets.top).unwrap() <= input[1]);
    }

    #[test]
    fn pixel_trim_fails_open_to_verified_uia_surface_when_no_edge_is_proven() {
        let width = 640;
        let height = 48;
        let visible = [0, 0, width, height];
        let input = [48, 8, 540, 40];
        let pixels = synthetic_surface(width, height, visible, input);
        assert_eq!(
            adaptive_surface_insets(&pixels, width, height, input),
            SurfaceInsets::default()
        );
    }

    #[test]
    fn capture_state_is_bound_to_exact_session_and_generation() {
        let state = NativeSurfaceCaptureState::default();
        let capture = NativeSurfaceCapture {
            version: "osl-native-surface-capture-v1",
            image_data_url: "data:image/bmp;base64,AA==".to_owned(),
            width_px: 320,
            height_px: 24,
            input_left_px: 0,
            input_top_px: 0,
            input_width_px: 320,
            input_height_px: 24,
            input_background: "#000000".to_owned(),
            text_left_px: 0,
            text_top_px: 0,
            text_width_px: 320,
            text_height_px: 24,
            font_family: None,
            font_size_px: None,
            font_weight: None,
            line_height_px: None,
            presentation_insets: SurfaceInsets::default(),
        };
        let key = NativeSurfaceKey {
            session_epoch: 7,
            host_generation: 11,
        };
        state.replace(key, capture.clone()).expect("store");
        assert_eq!(
            state
                .current(key)
                .expect("same key returns capture")
                .width_px,
            320
        );
        assert!(state
            .current(NativeSurfaceKey {
                session_epoch: 8,
                host_generation: 11,
            })
            .is_none());
        assert!(state
            .current(NativeSurfaceKey {
                session_epoch: 7,
                host_generation: 12,
            })
            .is_none());

        let replacement_key = NativeSurfaceKey {
            session_epoch: 7,
            host_generation: 12,
        };
        let mut replacement = capture;
        replacement.width_px = 640;
        state
            .replace(replacement_key, replacement)
            .expect("replace capture");
        assert!(state.current(key).is_none());
        assert_eq!(
            state
                .current(replacement_key)
                .expect("replacement key returns capture")
                .width_px,
            640
        );
        state.clear();
        assert!(state.current(replacement_key).is_none());
    }
}
