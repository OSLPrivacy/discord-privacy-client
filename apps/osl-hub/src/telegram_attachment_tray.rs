//! Telegram's local attachment picker state.
//!
//! Telegram only receives a file after the native send boundary has accepted
//! it.  This tray is deliberately just the picker-facing metadata: every
//! accepted row has a filename, MIME type, byte size, and a named remove
//! control.  Invalid selections never produce placeholder rows.

use std::fmt;

pub const TELEGRAM_MAX_ATTACHMENTS: usize = 16;
pub const TELEGRAM_MAX_ATTACHMENT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramPickedFile {
    pub name: String,
    pub media_type: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramAttachmentTrayRow {
    pub name: String,
    pub media_type: String,
    pub size: u64,
    /// The accessible, stable name of the row's remove control.
    pub remove_control: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TelegramAttachmentTrayError {
    EmptyName,
    InvalidType,
    InvalidSize,
    TooLarge { size: u64, max: u64 },
    TooManyFiles { requested: usize, max: usize },
    UnknownRemoveControl,
}

impl fmt::Display for TelegramAttachmentTrayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => formatter.write_str("Telegram attachment name is required"),
            Self::InvalidType => formatter.write_str("Telegram attachment type is required"),
            Self::InvalidSize => formatter.write_str("Telegram attachment size must be positive"),
            Self::TooLarge { size, max } => write!(formatter, "Telegram attachment is {size} bytes; maximum is {max}"),
            Self::TooManyFiles { requested, max } => write!(formatter, "Telegram attachment tray accepts {max} files, not {requested}"),
            Self::UnknownRemoveControl => formatter.write_str("Telegram attachment remove control was not found"),
        }
    }
}

impl std::error::Error for TelegramAttachmentTrayError {}

#[derive(Default)]
pub struct TelegramAttachmentTray {
    rows: Vec<TelegramAttachmentTrayRow>,
    next_control: u64,
}

impl TelegramAttachmentTray {
    /// Add a picker result atomically. A rejected file or over-capacity batch
    /// leaves the existing tray intact, preventing blank/partial UI rows.
    pub fn pick(
        &mut self,
        files: impl IntoIterator<Item = TelegramPickedFile>,
    ) -> Result<Vec<TelegramAttachmentTrayRow>, TelegramAttachmentTrayError> {
        let files = files.into_iter().collect::<Vec<_>>();
        let requested = self.rows.len().saturating_add(files.len());
        if requested > TELEGRAM_MAX_ATTACHMENTS {
            return Err(TelegramAttachmentTrayError::TooManyFiles {
                requested,
                max: TELEGRAM_MAX_ATTACHMENTS,
            });
        }
        for file in &files {
            validate(file)?;
        }

        let mut accepted = Vec::with_capacity(files.len());
        for file in files {
            self.next_control = self.next_control.saturating_add(1);
            let row = TelegramAttachmentTrayRow {
                name: file.name.trim().to_owned(),
                media_type: file.media_type.trim().to_owned(),
                size: file.size,
                remove_control: format!("Remove attachment {}", self.next_control),
            };
            self.rows.push(row.clone());
            accepted.push(row);
        }
        Ok(accepted)
    }

    pub fn rows(&self) -> &[TelegramAttachmentTrayRow] {
        &self.rows
    }

    pub fn remove(&mut self, remove_control: &str) -> Result<TelegramAttachmentTrayRow, TelegramAttachmentTrayError> {
        let index = self.rows.iter().position(|row| row.remove_control == remove_control)
            .ok_or(TelegramAttachmentTrayError::UnknownRemoveControl)?;
        Ok(self.rows.remove(index))
    }
}

fn validate(file: &TelegramPickedFile) -> Result<(), TelegramAttachmentTrayError> {
    if file.name.trim().is_empty() || file.name.trim() == "." || file.name.trim() == ".." {
        return Err(TelegramAttachmentTrayError::EmptyName);
    }
    if file.media_type.trim().is_empty() || !file.media_type.contains('/') {
        return Err(TelegramAttachmentTrayError::InvalidType);
    }
    if file.size == 0 {
        return Err(TelegramAttachmentTrayError::InvalidSize);
    }
    if file.size > TELEGRAM_MAX_ATTACHMENT_BYTES {
        return Err(TelegramAttachmentTrayError::TooLarge { size: file.size, max: TELEGRAM_MAX_ATTACHMENT_BYTES });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(name: &str, size: u64) -> TelegramPickedFile {
        TelegramPickedFile { name: name.to_owned(), media_type: "application/pdf".to_owned(), size }
    }

    #[test]
    fn task1022_a_picked_valid_file_has_all_four_named_controls() {
        let mut tray = TelegramAttachmentTray::default();
        let rows = tray.pick([valid("travel-plan.pdf", 8 * 1024 * 1024)]).unwrap();
        let row = &rows[0];
        println!(
            "TASK1022 telegram_attachment_tray rows={} name={} type={} size={} remove_control={}",
            rows.len(), row.name, row.media_type, row.size, row.remove_control
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(row.name, "travel-plan.pdf");
        assert_eq!(row.media_type, "application/pdf");
        assert_eq!(row.size, 8 * 1024 * 1024);
        assert_eq!(row.remove_control, "Remove attachment 1");
        assert!(tray.rows().iter().all(|entry| !entry.name.trim().is_empty()));
    }

    #[test]
    fn task1022_limits_the_tray_to_16_files_and_8_mib_each() {
        let mut tray = TelegramAttachmentTray::default();
        let files = (1..=TELEGRAM_MAX_ATTACHMENTS)
            .map(|number| valid(&format!("file-{number}.txt"), TELEGRAM_MAX_ATTACHMENT_BYTES))
            .collect::<Vec<_>>();
        assert_eq!(tray.pick(files).unwrap().len(), TELEGRAM_MAX_ATTACHMENTS);
        assert_eq!(tray.rows().len(), 16);
        assert_eq!(tray.pick([valid("seventeenth.txt", 1)]), Err(TelegramAttachmentTrayError::TooManyFiles { requested: 17, max: 16 }));

        let mut empty_tray = TelegramAttachmentTray::default();
        assert_eq!(empty_tray.pick([valid("too-large.txt", TELEGRAM_MAX_ATTACHMENT_BYTES + 1)]), Err(TelegramAttachmentTrayError::TooLarge { size: TELEGRAM_MAX_ATTACHMENT_BYTES + 1, max: TELEGRAM_MAX_ATTACHMENT_BYTES }));
        assert!(empty_tray.rows().is_empty());
    }

    #[test]
    fn task1022_invalid_picker_input_creates_zero_unnamed_placeholder_rows() {
        let mut tray = TelegramAttachmentTray::default();
        assert_eq!(tray.pick([valid("  ", 1)]), Err(TelegramAttachmentTrayError::EmptyName));
        println!("TASK1022 invalid_picker rows={} unnamed_placeholder_rows=0", tray.rows().len());
        assert!(tray.rows().is_empty());
    }
}
