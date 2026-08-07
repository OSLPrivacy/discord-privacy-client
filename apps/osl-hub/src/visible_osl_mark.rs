use serde::{Deserialize, Serialize};
use std::fmt;

pub const VISIBLE_OSL_MARK: &str = " [OSL]";

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisibleOslMarkAppView {
    DiscordProfileName,
}

impl VisibleOslMarkAppView {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DiscordProfileName => "discord-profile-name",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisibleOslMarkState {
    Visible,
    Silent,
}

impl VisibleOslMarkState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Silent => "silent",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibleOslMarkRequest {
    pub app_view: VisibleOslMarkAppView,
    pub plain_name: String,
    pub mark_state: VisibleOslMarkState,
}

impl VisibleOslMarkRequest {
    pub fn new(
        app_view: VisibleOslMarkAppView,
        plain_name: impl Into<String>,
        mark_state: VisibleOslMarkState,
    ) -> Self {
        Self {
            app_view,
            plain_name: plain_name.into(),
            mark_state,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisibleOslMarkCommandResult {
    pub app_view: VisibleOslMarkAppView,
    pub before: String,
    pub after: String,
    pub mark: &'static str,
    pub mark_state: VisibleOslMarkState,
    pub changed: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VisibleOslMarkError {
    EmptyName,
    MarkAbsent,
}

impl fmt::Display for VisibleOslMarkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => f.write_str("profile name is empty"),
            Self::MarkAbsent => f.write_str("visible OSL mark is absent"),
        }
    }
}

impl std::error::Error for VisibleOslMarkError {}

pub fn put_visible_osl_mark(
    request: &VisibleOslMarkRequest,
) -> Result<VisibleOslMarkCommandResult, VisibleOslMarkError> {
    if request.plain_name.is_empty() {
        return Err(VisibleOslMarkError::EmptyName);
    }

    let after = match request.mark_state {
        VisibleOslMarkState::Visible => name_with_mark(&request.plain_name),
        VisibleOslMarkState::Silent => request.plain_name.clone(),
    };

    Ok(VisibleOslMarkCommandResult {
        app_view: request.app_view,
        changed: after != request.plain_name,
        before: request.plain_name.clone(),
        after,
        mark: VISIBLE_OSL_MARK,
        mark_state: request.mark_state,
    })
}

pub fn take_visible_osl_mark_out(
    request: &VisibleOslMarkRequest,
) -> Result<VisibleOslMarkCommandResult, VisibleOslMarkError> {
    if request.plain_name.is_empty() {
        return Err(VisibleOslMarkError::EmptyName);
    }

    let after = match request.mark_state {
        VisibleOslMarkState::Visible => request
            .plain_name
            .strip_suffix(VISIBLE_OSL_MARK)
            .ok_or(VisibleOslMarkError::MarkAbsent)?
            .to_owned(),
        VisibleOslMarkState::Silent => request.plain_name.clone(),
    };

    Ok(VisibleOslMarkCommandResult {
        app_view: request.app_view,
        changed: after != request.plain_name,
        before: request.plain_name.clone(),
        after,
        mark: VISIBLE_OSL_MARK,
        mark_state: request.mark_state,
    })
}

fn name_with_mark(plain_name: &str) -> String {
    if plain_name.ends_with(VISIBLE_OSL_MARK) {
        plain_name.to_owned()
    } else {
        format!("{plain_name}{VISIBLE_OSL_MARK}")
    }
}
