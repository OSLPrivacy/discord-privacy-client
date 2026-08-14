//! TASK 6844 helper: press PrintScreen, from a different process.
//!
//! The check must not press the key inside the process that is watching for
//! it, or the "detection" would only prove that a program can hear itself. A
//! separate process presses the key, Windows performs the capture, and the
//! watcher sees the bitmap Windows produced.

#[cfg(windows)]
fn main() {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        keybd_event, KEYEVENTF_KEYUP, VK_SNAPSHOT,
    };

    unsafe {
        keybd_event(VK_SNAPSHOT as u8, 0, 0, 0);
        keybd_event(VK_SNAPSHOT as u8, 0, KEYEVENTF_KEYUP, 0);
    }
    // Give Windows time to finish writing the clipboard before this process
    // exits and the parent stops waiting on it.
    std::thread::sleep(std::time::Duration::from_millis(500));
}

#[cfg(not(windows))]
fn main() {
    eprintln!("TASK 6844: pressing PrintScreen requires Windows");
    std::process::exit(1);
}
