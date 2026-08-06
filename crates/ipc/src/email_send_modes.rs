#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailSendMode {
    Manual,
    DoubleEnter,
    ExperimentalSingleEnter,
    Instant,
    MatchTyping,
}

impl EmailSendMode {
    pub const ALL: [Self; 5] = [
        Self::Manual,
        Self::DoubleEnter,
        Self::ExperimentalSingleEnter,
        Self::Instant,
        Self::MatchTyping,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::DoubleEnter => "double_enter",
            Self::ExperimentalSingleEnter => "experimental_single_enter",
            Self::Instant => "instant",
            Self::MatchTyping => "match_typing",
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::DoubleEnter => "Double Enter",
            Self::ExperimentalSingleEnter => "Experimental Single Enter",
            Self::Instant => "Instant",
            Self::MatchTyping => "Match typing",
        }
    }
}
