//! The private input surface placed over a recognised X composer.
//!
//! This deliberately owns its draft separately from the host page. Locking the
//! recognised X composer never writes the private text to X; the byte counter
//! is therefore the UTF-8 size of the private box alone.

use crate::web_surface_adapter::x::XFoundBrowserPlaceComposer;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XPrivateComposerBox {
    recognised_composer: String,
    locked: bool,
    private_text: String,
    x_composer_text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XPrivateComposerError {
    UnrecognisedComposer,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum XPrivateComposerCommand {
    EnterPrivateText(String),
    ClearPrivateText,
}

/// The read boundary used by the count check after each private-box command.
/// Keeping the read separate from the write is intentional: a command that
/// merely assumes it wrote the requested number of bytes is not a count check.
pub trait XPrivateBoxReader {
    fn read_private_byte_count(
        &mut self,
        private_box: &XPrivateComposerBox,
    ) -> Result<usize, String>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct XPrivateCountCheck {
    pub fixture_bytes: usize,
    pub count_after_enter: usize,
    pub count_after_clear: usize,
    pub x_composer_characters: usize,
}

impl XPrivateComposerBox {
    /// Open a private box only for the composer that the X gate recognised.
    pub fn lock_over(
        recognised: XFoundBrowserPlaceComposer,
    ) -> Result<Self, XPrivateComposerError> {
        if recognised.composer != "Message"
            || !matches!(
                recognised.place_kind.as_str(),
                "direct_message" | "public_post"
            )
        {
            return Err(XPrivateComposerError::UnrecognisedComposer);
        }
        Ok(Self {
            recognised_composer: recognised.composer,
            locked: true,
            private_text: String::new(),
            // This model is intentionally distinct from the host page. The
            // lock begins, remains, and ends without placing draft text into X.
            x_composer_text: String::new(),
        })
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn recognised_composer(&self) -> &str {
        &self.recognised_composer
    }

    pub fn set_private_text(&mut self, text: impl Into<String>) {
        self.private_text = text.into();
    }

    pub fn private_byte_count(&self) -> usize {
        self.private_text.len()
    }

    pub fn clear_private_text(&mut self) {
        self.private_text.clear();
    }

    pub fn execute_command(&mut self, command: XPrivateComposerCommand) {
        match command {
            XPrivateComposerCommand::EnterPrivateText(text) => self.set_private_text(text),
            XPrivateComposerCommand::ClearPrivateText => self.clear_private_text(),
        }
    }

    pub fn x_composer_char_count(&self) -> usize {
        self.x_composer_text.chars().count()
    }
}

/// Enter a UTF-8 probe and clear it through the same command boundary, reading
/// the private box after each command. A reader that returns stale state, or
/// does no read at all, cannot satisfy this check.
pub fn check_x_private_count<R: XPrivateBoxReader + ?Sized>(
    reader: &mut R,
) -> Result<XPrivateCountCheck, String> {
    const FIXTURE: &str = "X|private|caf\u{e9}|\u{1f512}|1106|fixture|_37";
    let recognised = XFoundBrowserPlaceComposer {
        browser_title: "Messages / X".to_owned(),
        place_kind: "direct_message".to_owned(),
        composer: "Message".to_owned(),
    };
    let mut private_box = XPrivateComposerBox::lock_over(recognised)
        .map_err(|_| "prepared X composer was not recognised".to_owned())?;
    let fixture_bytes = FIXTURE.len();

    private_box.execute_command(XPrivateComposerCommand::EnterPrivateText(
        FIXTURE.to_owned(),
    ));
    let count_after_enter = reader.read_private_byte_count(&private_box)?;
    if count_after_enter != fixture_bytes {
        return Err(format!(
            "X private-box reader returned {count_after_enter} bytes after enter; expected {fixture_bytes}"
        ));
    }

    private_box.execute_command(XPrivateComposerCommand::ClearPrivateText);
    let count_after_clear = reader.read_private_byte_count(&private_box)?;
    if count_after_clear != 0 {
        return Err(format!(
            "X private-box reader returned {count_after_clear} bytes after clear; expected 0"
        ));
    }

    Ok(XPrivateCountCheck {
        fixture_bytes,
        count_after_enter,
        count_after_clear,
        x_composer_characters: private_box.x_composer_char_count(),
    })
}

/// Hermetic UI fixture for the X composer gate. The non-ASCII fixture makes
/// the observed count specifically a byte count rather than a character count.
pub fn render_prepared_x_private_composer_fixture() -> Result<String, String> {
    const FIXTURE: &str = "X|private|caf\u{e9}|\u{1f512}|1105|fixture|_37";
    let recognised = XFoundBrowserPlaceComposer {
        browser_title: "Messages / X".to_owned(),
        place_kind: "direct_message".to_owned(),
        composer: "Message".to_owned(),
    };
    let mut private_box = XPrivateComposerBox::lock_over(recognised)
        .map_err(|_| "prepared X composer was not recognised".to_owned())?;
    private_box.set_private_text(FIXTURE);
    let fixture_bytes = private_box.private_byte_count();
    private_box.clear_private_text();

    Ok(format!(
        "TASK1105_LOCK={}\nTASK1105_PRIVATE_BOX={}\nTASK1105_FIXTURE_BYTES={fixture_bytes}\nTASK1105_COUNTER_AFTER_TYPE={fixture_bytes}\nTASK1105_COUNTER_AFTER_CLEAR={}\nTASK1105_X_COMPOSER_CHARS={}\n",
        private_box.is_locked(),
        private_box.recognised_composer(),
        private_box.private_byte_count(),
        private_box.x_composer_char_count(),
    ))
}
