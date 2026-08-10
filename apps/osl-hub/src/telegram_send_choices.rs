/// The only actions that may finish a prepared Telegram send.
///
/// Parsing is deliberately exact and fail-closed: choices used by other
/// providers or older preference screens must not silently acquire Telegram
/// send authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelegramSendTrigger {
    Enter,
    EnterX2,
    Clipboard,
}

impl TelegramSendTrigger {
    pub const ALL: [Self; 3] = [Self::Enter, Self::EnterX2, Self::Clipboard];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Enter => "Enter",
            Self::EnterX2 => "Enter x2",
            Self::Clipboard => "Clipboard",
        }
    }

    pub fn parse(input: &str) -> Result<Self, String> {
        match input {
            "Enter" => Ok(Self::Enter),
            "Enter x2" => Ok(Self::EnterX2),
            "Clipboard" => Ok(Self::Clipboard),
            _ => Err(format!(
                "OSL: Telegram send refused: unsupported trigger '{input}'"
            )),
        }
    }
}

/// How the already-prepared public cover is inserted into Telegram.
///
/// This is intentionally not a send trigger. It remains a separate typed
/// choice on the preparation receipt so selecting an insertion style cannot
/// select or imply the action that sends the message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TelegramCoverInsertion {
    InsertOnSend,
    TypeNaturally,
}

impl TelegramCoverInsertion {
    pub const ALL: [Self; 2] = [Self::InsertOnSend, Self::TypeNaturally];

    pub const fn label(self) -> &'static str {
        match self {
            Self::InsertOnSend => "Insert on send",
            Self::TypeNaturally => "Type naturally",
        }
    }
}

/// Direct, side-effect-free preparation result consumed by the Telegram send
/// driver. The cover remains raw bytes because preparation must preserve it
/// even when it is not valid UTF-8.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramSendPreparationReceipt {
    pub trigger: TelegramSendTrigger,
    pub cover_insertion: TelegramCoverInsertion,
    pub cover: Vec<u8>,
}

pub fn prepare_telegram_send(
    trigger: &str,
    cover_insertion: TelegramCoverInsertion,
    cover: &[u8],
) -> Result<TelegramSendPreparationReceipt, String> {
    Ok(TelegramSendPreparationReceipt {
        trigger: TelegramSendTrigger::parse(trigger)?,
        cover_insertion,
        cover: cover.to_vec(),
    })
}

/// UI-facing state for Telegram's send button.
///
/// Pressing this control only prepares the selected cover and cannot post to
/// Telegram. The placement flow remains responsible for inserting the cover
/// into a Telegram composer later.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TelegramSendButton {
    selected_trigger: TelegramSendTrigger,
    selected_cover_insertion: TelegramCoverInsertion,
}

impl TelegramSendButton {
    /// Connect the button to the exact trigger and separately selected cover
    /// insertion choice shown by the Telegram composer.
    pub fn for_selected_choices(
        trigger: &str,
        cover_insertion: TelegramCoverInsertion,
    ) -> Result<Self, String> {
        Ok(Self {
            selected_trigger: TelegramSendTrigger::parse(trigger)?,
            selected_cover_insertion: cover_insertion,
        })
    }

    /// Prepare the selected cover without posting it to Telegram.
    pub fn prepare_selected_cover(
        &self,
        cover: &[u8],
    ) -> Result<TelegramSendPreparationReceipt, String> {
        prepare_telegram_send(
            self.selected_trigger.label(),
            self.selected_cover_insertion,
            cover,
        )
    }
}
