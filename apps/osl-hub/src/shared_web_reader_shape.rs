//! Shared web-reader shape for X, Instagram, and Messenger.
//!
//! This is a contract shape only. Provider adapters must fill the internals
//! later; every declared job stays explicitly not built until then.

use crate::row_who_wrote_it::SharedRowWhoWroteIt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedWebReaderApp {
    X,
    Instagram,
    Messenger,
}

impl SharedWebReaderApp {
    pub const ALL: [Self; 3] = [Self::X, Self::Instagram, Self::Messenger];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Instagram => "instagram",
            Self::Messenger => "messenger",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SharedWebReaderRow {
    pub message_text: String,
    pub who_wrote_it: SharedRowWhoWroteIt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SharedWebReaderJobAnswer {
    NotBuiltYet,
}

impl SharedWebReaderJobAnswer {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotBuiltYet => "not built yet",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedWebReaderJob {
    pub name: &'static str,
    pub answer: SharedWebReaderJobAnswer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SharedWebReaderShape {
    pub app: SharedWebReaderApp,
    pub jobs: &'static [SharedWebReaderJob],
    pub refusals: &'static [&'static str],
    pub scrolled_list_cancels_paint: bool,
}

const NOT_BUILT_YET: SharedWebReaderJobAnswer = SharedWebReaderJobAnswer::NotBuiltYet;

pub const SHARED_WEB_READER_JOBS: [SharedWebReaderJob; 5] = [
    SharedWebReaderJob {
        name: "read_visible_rows",
        answer: NOT_BUILT_YET,
    },
    SharedWebReaderJob {
        name: "read_row_message_text",
        answer: NOT_BUILT_YET,
    },
    SharedWebReaderJob {
        name: "read_row_who_wrote_it",
        answer: NOT_BUILT_YET,
    },
    SharedWebReaderJob {
        name: "scroll_message_list",
        answer: NOT_BUILT_YET,
    },
    SharedWebReaderJob {
        name: "paint_protected_rows",
        answer: NOT_BUILT_YET,
    },
];

pub const SHARED_WEB_READER_REFUSALS: [&str; 6] = [
    "OSL: web reader is not built yet",
    "OSL: sign in yourself",
    "OSL: row message text is missing",
    "OSL: row has no who-wrote-it evidence",
    "OSL: row was not published by the app",
    "OSL: scrolled list cancels paint",
];

pub const SHARED_WEB_READER_SHAPE: SharedWebReaderShape = SharedWebReaderShape {
    app: SharedWebReaderApp::X,
    jobs: &SHARED_WEB_READER_JOBS,
    refusals: &SHARED_WEB_READER_REFUSALS,
    scrolled_list_cancels_paint: true,
};

pub const fn shared_web_reader_shape_for(app: SharedWebReaderApp) -> SharedWebReaderShape {
    SharedWebReaderShape {
        app,
        ..SHARED_WEB_READER_SHAPE
    }
}
