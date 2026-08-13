//! Desktop implementation of the recovery-kit picker port.
//!
//! This module deliberately owns no parser and no importer.  It returns only
//! the path selected by the installed dialog; `recovery_kit_file` immediately
//! freezes that path into one byte buffer before it is inspected.

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;

use crate::recovery_kit_file::{RecoveryKitPicker, RecoveryKitRefusal, RECOVERY_KIT_EXTENSION};

pub struct DesktopRecoveryKitPicker {
    app: AppHandle,
}

impl DesktopRecoveryKitPicker {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl RecoveryKitPicker for DesktopRecoveryKitPicker {
    fn pick(&self) -> Result<Option<std::path::PathBuf>, RecoveryKitRefusal> {
        let parent = self
            .app
            .get_webview_window("main")
            .ok_or(RecoveryKitRefusal::Unreadable)?;
        let selected = self
            .app
            .dialog()
            .file()
            .set_parent(&parent)
            .set_title("Choose an OSL recovery kit")
            .add_filter("OSL recovery kit", &[RECOVERY_KIT_EXTENSION])
            .blocking_pick_file();
        selected
            .map(|file| file.into_path().map_err(|_| RecoveryKitRefusal::Unreadable))
            .transpose()
    }
}
