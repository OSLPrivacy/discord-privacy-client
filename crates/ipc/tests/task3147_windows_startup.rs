// Compile the desktop startup module through IPC's focused test harness. This
// avoids building the unrelated Tauri application while exercising the exact
// Run-key synchronization policy used by the desktop command.
#[path = "../../../src-tauri/src/windows_startup.rs"]
mod windows_startup;
