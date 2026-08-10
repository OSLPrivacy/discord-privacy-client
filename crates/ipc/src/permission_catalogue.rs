use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforcementTag {
    Key,
    Relay,
    Trust,
}

impl EnforcementTag {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Key => "KEY",
            Self::Relay => "RELAY",
            Self::Trust => "TRUST",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "KEY" => Some(Self::Key),
            "RELAY" => Some(Self::Relay),
            "TRUST" => Some(Self::Trust),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionRow {
    pub section: &'static str,
    pub words: &'static str,
    pub tag: EnforcementTag,
}

pub const SECTION_NAMES: [&str; 7] = [
    "seeing",
    "talking",
    "threads",
    "voice",
    "moderation",
    "managing",
    "appearance",
];

pub const PERMISSION_CATALOGUE: [PermissionRow; 40] = [
    PermissionRow {
        section: "seeing",
        words: "read a text channel",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "seeing",
        words: "read channel history",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "seeing",
        words: "read message reactions",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "seeing",
        words: "read attached file names",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "seeing",
        words: "see the member list",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "seeing",
        words: "see audit log entries",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "talking",
        words: "send a message",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "talking",
        words: "edit your own message",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "talking",
        words: "delete your own message",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "talking",
        words: "attach pictures",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "talking",
        words: "attach files",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "talking",
        words: "add reactions",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "talking",
        words: "mention everyone",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "talking",
        words: "use external emoji",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "threads",
        words: "create a thread",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "threads",
        words: "read a thread",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "threads",
        words: "send in a thread",
        tag: EnforcementTag::Key,
    },
    PermissionRow {
        section: "threads",
        words: "rename a thread",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "threads",
        words: "archive a thread",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "voice",
        words: "join voice",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "voice",
        words: "speak in voice",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "voice",
        words: "share screen in voice",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "voice",
        words: "move self between voice channels",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "voice",
        words: "use voice activity",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "moderation",
        words: "timeout a member",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "moderation",
        words: "kick a member",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "moderation",
        words: "ban a member",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "moderation",
        words: "delete another member's message",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "moderation",
        words: "request clients hide a delivered message",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "moderation",
        words: "pin or unpin a message",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "moderation",
        words: "review reported messages",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "managing",
        words: "manage roles",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "managing",
        words: "manage channels",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "managing",
        words: "manage invite links",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "managing",
        words: "create invite links",
        tag: EnforcementTag::Relay,
    },
    PermissionRow {
        section: "managing",
        words: "change member nicknames",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "managing",
        words: "manage server webhooks",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "appearance",
        words: "change enclave appearance",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "appearance",
        words: "change role appearance",
        tag: EnforcementTag::Trust,
    },
    PermissionRow {
        section: "appearance",
        words: "change channel appearance",
        tag: EnforcementTag::Relay,
    },
];

pub const REQUIRED_PERMISSION_ROWS: [&str; 13] = [
    "read a text channel",
    "send a message",
    "attach pictures",
    "mention everyone",
    "create a thread",
    "join voice",
    "timeout a member",
    "request clients hide a delivered message",
    "manage roles",
    "manage channels",
    "manage invite links",
    "change enclave appearance",
    "change role appearance",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionCatalogueReport {
    pub section_names: usize,
    pub permission_rows: usize,
    pub enforcement_tags: usize,
    pub tags: Vec<&'static str>,
    pub required_rows: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionCatalogueError {
    MissingTag {
        row: String,
    },
    UnknownTag {
        row: String,
        tag: String,
    },
    WrongTag {
        row: String,
        expected: &'static str,
        actual: &'static str,
    },
    WrongSectionCount {
        actual: usize,
    },
    WrongRowCount {
        actual: usize,
    },
    WrongTagCount {
        actual: usize,
    },
    MissingRequiredRow {
        row: &'static str,
    },
    RowOutsideSection {
        row: String,
    },
    Problems {
        messages: Vec<String>,
    },
}

impl fmt::Display for PermissionCatalogueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTag { row } => write!(f, "row has no enforcement tag: {row}"),
            Self::UnknownTag { row, tag } => {
                write!(f, "row has unknown enforcement tag `{tag}`: {row}")
            }
            Self::WrongTag {
                row,
                expected,
                actual,
            } => write!(
                f,
                "row is wrongly tagged: {row} expected `{expected}`, found `{actual}`"
            ),
            Self::WrongSectionCount { actual } => {
                write!(f, "expected 7 section names, found {actual}")
            }
            Self::WrongRowCount { actual } => {
                write!(f, "expected 40 permission rows, found {actual}")
            }
            Self::WrongTagCount { actual } => {
                write!(f, "expected 40 enforcement tags, found {actual}")
            }
            Self::MissingRequiredRow { row } => write!(f, "missing required permission row: {row}"),
            Self::RowOutsideSection { row } => {
                write!(f, "row appears before a section name: {row}")
            }
            Self::Problems { messages } => write!(f, "{}", messages.join("; ")),
        }
    }
}

impl std::error::Error for PermissionCatalogueError {}

pub fn render_permission_catalogue() -> String {
    let mut rendered = String::new();
    for section in SECTION_NAMES {
        rendered.push_str(section);
        rendered.push('\n');
        for row in PERMISSION_CATALOGUE
            .iter()
            .filter(|permission| permission.section == section)
        {
            rendered.push_str("- ");
            rendered.push_str(row.words);
            rendered.push_str(" `");
            rendered.push_str(row.tag.as_str());
            rendered.push_str("`\n");
        }
    }
    rendered
}

pub fn check_permission_catalogue_text(
    text: &str,
) -> Result<PermissionCatalogueReport, PermissionCatalogueError> {
    let mut section_names = 0;
    let mut permission_rows = 0;
    let mut enforcement_tags = 0;
    let mut tags = Vec::new();
    let mut rows = Vec::new();
    let mut in_section = false;
    let mut problems = Vec::new();

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if SECTION_NAMES.contains(&line) {
            section_names += 1;
            in_section = true;
            continue;
        }

        if let Some(row) = line.strip_prefix("- ") {
            if !in_section {
                problems.push(
                    PermissionCatalogueError::RowOutsideSection {
                        row: row.to_owned(),
                    }
                    .to_string(),
                );
                continue;
            }
            permission_rows += 1;
            let words = row_words(row);
            rows.push(words);
            match parse_tagged_row(row) {
                Ok((words, tag)) => {
                    tags.push(tag.as_str());
                    enforcement_tags += 1;
                    if let Some(expected) = expected_tag_for_row(words) {
                        if tag != expected {
                            problems.push(
                                PermissionCatalogueError::WrongTag {
                                    row: words.to_owned(),
                                    expected: expected.as_str(),
                                    actual: tag.as_str(),
                                }
                                .to_string(),
                            );
                        }
                    }
                }
                Err(error) => problems.push(error.to_string()),
            }
        }
    }

    if section_names != SECTION_NAMES.len() {
        problems.push(
            PermissionCatalogueError::WrongSectionCount {
                actual: section_names,
            }
            .to_string(),
        );
    }
    if permission_rows != PERMISSION_CATALOGUE.len() {
        problems.push(
            PermissionCatalogueError::WrongRowCount {
                actual: permission_rows,
            }
            .to_string(),
        );
    }
    if enforcement_tags != PERMISSION_CATALOGUE.len() {
        problems.push(
            PermissionCatalogueError::WrongTagCount {
                actual: enforcement_tags,
            }
            .to_string(),
        );
    }
    for row in REQUIRED_PERMISSION_ROWS {
        if !rows.contains(&row) {
            problems.push(PermissionCatalogueError::MissingRequiredRow { row }.to_string());
        }
    }
    if !problems.is_empty() {
        return Err(PermissionCatalogueError::Problems { messages: problems });
    }

    Ok(PermissionCatalogueReport {
        section_names,
        permission_rows,
        enforcement_tags,
        tags,
        required_rows: REQUIRED_PERMISSION_ROWS.to_vec(),
    })
}

fn row_words(row: &str) -> &str {
    row.rsplit_once(" `")
        .map(|(words, _)| words.trim())
        .unwrap_or_else(|| row.trim())
}

fn expected_tag_for_row(row: &str) -> Option<EnforcementTag> {
    PERMISSION_CATALOGUE
        .iter()
        .find(|permission| permission.words == row)
        .map(|permission| permission.tag)
}

fn parse_tagged_row(row: &str) -> Result<(&str, EnforcementTag), PermissionCatalogueError> {
    let Some((words, tag_suffix)) = row.rsplit_once(" `") else {
        return Err(PermissionCatalogueError::MissingTag {
            row: row.trim().to_owned(),
        });
    };
    let Some(tag) = tag_suffix.strip_suffix('`') else {
        return Err(PermissionCatalogueError::MissingTag {
            row: row.trim().to_owned(),
        });
    };
    let Some(tag) = EnforcementTag::parse(tag) else {
        return Err(PermissionCatalogueError::UnknownTag {
            row: words.trim().to_owned(),
            tag: tag.to_owned(),
        });
    };
    Ok((words.trim(), tag))
}
