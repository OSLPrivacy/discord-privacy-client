//! TASK 6844 helper: a real screen capture through a path OSL cannot detect.
//!
//! This is what a screen recorder, a remote-desktop client or any of the
//! hundreds of screenshot utilities actually do: get the screen DC, BitBlt it
//! into your own process, write it wherever you like. No key is pressed and
//! nothing touches the clipboard, so Windows has nothing to report and neither
//! does OSL.
//!
//! The check runs this while a view-once viewer is open and proves that the
//! capture really happened and that no notification followed. That pairing is
//! the honest version of the feature: the limitation is demonstrated, not just
//! written down.
//!
//! Usage: `task-6844-unsupported-grab <output.bmp>`

#[cfg(windows)]
fn main() {
    use std::io::Write as _;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    };

    let Some(output) = std::env::args().nth(1) else {
        eprintln!("usage: task-6844-unsupported-grab <output.bmp>");
        std::process::exit(2);
    };

    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    if width <= 0 || height <= 0 {
        eprintln!("TASK 6844: no virtual screen to grab");
        std::process::exit(1);
    }

    let stride = (width as usize) * 4;
    let mut pixels = vec![0u8; stride * height as usize];
    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    if screen.is_null() {
        eprintln!("TASK 6844: no screen device context");
        std::process::exit(1);
    }
    let memory = unsafe { CreateCompatibleDC(screen) };
    let bitmap = unsafe { CreateCompatibleBitmap(screen, width, height) };
    let previous = unsafe { SelectObject(memory, bitmap) };
    let copied = unsafe { BitBlt(memory, 0, 0, width, height, screen, 0, 0, SRCCOPY) } != 0;
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
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
        eprintln!("TASK 6844: the out-of-process grab read {rows} of {height} rows");
        std::process::exit(1);
    }

    let distinct: std::collections::HashSet<[u8; 3]> = pixels
        .chunks_exact(4)
        .step_by(997)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    let file_size = 54 + pixels.len();
    let mut bmp = Vec::with_capacity(file_size);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(file_size as u32).to_le_bytes());
    bmp.extend_from_slice(&[0u8; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&height.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&32u16.to_le_bytes());
    bmp.extend_from_slice(&0u32.to_le_bytes());
    bmp.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
    bmp.extend_from_slice(&[0u8; 16]);
    bmp.extend_from_slice(&pixels);

    let mut file = match std::fs::File::create(&output) {
        Ok(file) => file,
        Err(error) => {
            eprintln!("TASK 6844: cannot write {output}: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = file.write_all(&bmp) {
        eprintln!("TASK 6844: cannot write {output}: {error}");
        std::process::exit(1);
    }
    println!(
        "unsupported_grab bytes={} width={width} height={height} distinct_sampled_colors={}",
        bmp.len(),
        distinct.len()
    );
}

#[cfg(not(windows))]
fn main() {
    eprintln!("TASK 6844: the out-of-process grab requires Windows");
    std::process::exit(1);
}
