use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadMessageRuleChoice {
    pub id: &'static str,
    pub label: &'static str,
    pub explanation: &'static str,
}

const BAD_MESSAGE_RULE_CHOICES: [BadMessageRuleChoice; 6] = [
    BadMessageRuleChoice {
        id: "passwords_and_codes",
        label: "Passwords and codes",
        explanation:
            "Login passwords, one-time codes, recovery phrases, PINs, or invite codes that could let someone into an account.",
    },
    BadMessageRuleChoice {
        id: "personal_details",
        label: "Personal details",
        explanation:
            "Names, addresses, phone numbers, locations, IDs, health details, or other facts that identify a person.",
    },
    BadMessageRuleChoice {
        id: "money_details",
        label: "Money details",
        explanation:
            "Card numbers, bank details, invoices, tax details, account balances, or payment information.",
    },
    BadMessageRuleChoice {
        id: "private_words",
        label: "Private words",
        explanation:
            "Words or names you add yourself, like a project name, nickname, or phrase you do not want left in messages.",
    },
    BadMessageRuleChoice {
        id: "private_pictures",
        label: "Private pictures",
        explanation:
            "Photos, screenshots, scans, or attachments that may show people, documents, rooms, screens, or other private things.",
    },
    BadMessageRuleChoice {
        id: "everything_above",
        label: "Everything above",
        explanation: "Use all of these rules together.",
    },
];

pub fn list_bad_message_rules() -> Vec<BadMessageRuleChoice> {
    BAD_MESSAGE_RULE_CHOICES.to_vec()
}

// Persistent selections for a local bad-message review run.
//
// The scanner can suggest matches, but the stored decision remains a possible
// match so downstream deletion/review flows cannot treat a rule hit as proof.

const STORE_DIR: &str = "bad-message-rules-v1";
const MAX_RUN_ID_BYTES: usize = 64;
const MAX_RULE_ID_BYTES: usize = 64;
const MAX_PRIVATE_WORD_BYTES: usize = 96;
const MAX_RULES: usize = 16;
const MAX_PRIVATE_WORDS: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BadMessageMatchTreatment {
    PossibleMatch,
}

impl BadMessageMatchTreatment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PossibleMatch => "possible_match",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BadMessageRunSelection {
    pub run_id: String,
    pub selected_rules: Vec<String>,
    pub private_words: Vec<String>,
    pub match_treatment: BadMessageMatchTreatment,
}

impl BadMessageRunSelection {
    pub fn new(
        run_id: impl Into<String>,
        selected_rules: Vec<String>,
        private_words: Vec<String>,
    ) -> Result<Self, String> {
        let selection = Self {
            run_id: run_id.into(),
            selected_rules,
            private_words,
            match_treatment: BadMessageMatchTreatment::PossibleMatch,
        };
        selection.validate()?;
        Ok(selection)
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_token(&self.run_id, MAX_RUN_ID_BYTES, "bad-message run id")?;
        validate_list(
            &self.selected_rules,
            MAX_RULES,
            MAX_RULE_ID_BYTES,
            "bad-message rule",
        )?;
        validate_list(
            &self.private_words,
            MAX_PRIVATE_WORDS,
            MAX_PRIVATE_WORD_BYTES,
            "private word",
        )?;
        if self.match_treatment != BadMessageMatchTreatment::PossibleMatch {
            return Err("bad-message matches must be stored as possible matches".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadMessageRunSelectionReceipt {
    pub run_id: String,
    pub saved_rule_count: usize,
    pub saved_private_word_count: usize,
    pub read_rule_count: usize,
    pub read_private_word_count: usize,
    pub match_treatment: BadMessageMatchTreatment,
}

pub fn save_bad_message_run_selection(
    root: &Path,
    selection: &BadMessageRunSelection,
) -> Result<BadMessageRunSelectionReceipt, String> {
    selection.validate()?;
    let path = selection_path(root, &selection.run_id)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create bad-message selection store: {error}"))?;
    }
    let body = serde_json::to_vec_pretty(selection)
        .map_err(|error| format!("serialize bad-message selection: {error}"))?;
    let sealed = ipc::main_password::maybe_encrypt(&body)
        .map_err(|error| format!("encrypt bad-message selection: {error}"))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, sealed)
        .map_err(|error| format!("write bad-message selection temp file: {error}"))?;
    std::fs::rename(&tmp, &path)
        .map_err(|error| format!("commit bad-message selection file: {error}"))?;

    let read_back = read_bad_message_run_selection(root, &selection.run_id)?
        .ok_or_else(|| "bad-message selection was not readable after save".to_owned())?;
    Ok(BadMessageRunSelectionReceipt {
        run_id: selection.run_id.clone(),
        saved_rule_count: selection.selected_rules.len(),
        saved_private_word_count: selection.private_words.len(),
        read_rule_count: read_back.selected_rules.len(),
        read_private_word_count: read_back.private_words.len(),
        match_treatment: read_back.match_treatment,
    })
}

pub fn read_bad_message_run_selection(
    root: &Path,
    run_id: &str,
) -> Result<Option<BadMessageRunSelection>, String> {
    let path = selection_path(root, run_id)?;
    let Ok(sealed) = std::fs::read(&path) else {
        return Ok(None);
    };
    let plain = ipc::main_password::maybe_decrypt_file(&path, &sealed)
        .map_err(|error| format!("decrypt bad-message selection: {error}"))?;
    let selection: BadMessageRunSelection = serde_json::from_slice(&plain)
        .map_err(|error| format!("parse bad-message selection: {error}"))?;
    selection.validate()?;
    Ok(Some(selection))
}

fn selection_path(root: &Path, run_id: &str) -> Result<PathBuf, String> {
    validate_token(run_id, MAX_RUN_ID_BYTES, "bad-message run id")?;
    Ok(root.join(STORE_DIR).join(format!("{run_id}.json")))
}

fn validate_list(
    values: &[String],
    max_count: usize,
    max_bytes: usize,
    name: &str,
) -> Result<(), String> {
    if values.is_empty() || values.len() > max_count {
        return Err(format!("{name} selection count is invalid"));
    }
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        validate_token(value, max_bytes, name)?;
        if !seen.insert(value) {
            return Err(format!("{name} selections must be unique"));
        }
    }
    Ok(())
}

fn validate_token(value: &str, max_bytes: usize, name: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > max_bytes
        || value
            .bytes()
            .any(|byte| !(byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')))
    {
        return Err(format!("{name} is invalid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::list_bad_message_rules;

    #[test]
    fn bad_message_rules_command_lists_all_six_choices_with_plain_explanations() {
        let rules = list_bad_message_rules();
        println!("list_bad_message_rules -> {} choices", rules.len());
        for rule in &rules {
            println!("{}: {}", rule.label, rule.explanation);
        }

        assert_eq!(rules.len(), 6);
        assert_eq!(rules[0].label, "Passwords and codes");
        assert_eq!(
            rules[0].explanation,
            "Login passwords, one-time codes, recovery phrases, PINs, or invite codes that could let someone into an account."
        );
        assert_eq!(rules[1].label, "Personal details");
        assert_eq!(
            rules[1].explanation,
            "Names, addresses, phone numbers, locations, IDs, health details, or other facts that identify a person."
        );
        assert_eq!(rules[2].label, "Money details");
        assert_eq!(
            rules[2].explanation,
            "Card numbers, bank details, invoices, tax details, account balances, or payment information."
        );
        assert_eq!(rules[3].label, "Private words");
        assert_eq!(
            rules[3].explanation,
            "Words or names you add yourself, like a project name, nickname, or phrase you do not want left in messages."
        );
        assert_eq!(rules[4].label, "Private pictures");
        assert_eq!(
            rules[4].explanation,
            "Photos, screenshots, scans, or attachments that may show people, documents, rooms, screens, or other private things."
        );
        assert_eq!(rules[5].label, "Everything above");
        assert_eq!(rules[5].explanation, "Use all of these rules together.");

        assert!(rules.iter().all(|rule| !rule.id.is_empty()));
        assert!(rules.iter().all(|rule| !rule.explanation.is_empty()));
    use super::*;
    }


    #[test]
    fn task_1412_saves_two_rules_and_two_words_as_possible_matches() {
        let temp = tempfile::tempdir().expect("tempdir");
        let selection = BadMessageRunSelection::new(
            "task-1412-run",
            vec!["harassment".to_owned(), "credential_leak".to_owned()],
            vec!["project-bluebird".to_owned(), "launch-code-17".to_owned()],
        )
        .expect("selection");

        ipc::main_password::set_file_storage_key(Some([0x14; 32]));
        let receipt = save_bad_message_run_selection(temp.path(), &selection).expect("save");
        let read_back = read_bad_message_run_selection(temp.path(), "task-1412-run")
            .expect("read")
            .expect("selection exists");

        println!(
            "TASK1412_TEST saved_rule_count={} saved_word_count={} read_rule_count={} read_word_count={} match_treatment={}",
            receipt.saved_rule_count,
            receipt.saved_private_word_count,
            receipt.read_rule_count,
            receipt.read_private_word_count,
            receipt.match_treatment.as_str()
        );
        assert_eq!(receipt.saved_rule_count, 2);
        assert_eq!(receipt.saved_private_word_count, 2);
        assert_eq!(read_back, selection);
        assert_eq!(
            read_back.match_treatment,
            BadMessageMatchTreatment::PossibleMatch
        );
    }
}
