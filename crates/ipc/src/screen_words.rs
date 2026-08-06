use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const EN_WORDS: &str = include_str!("screen_words/en.json");
const ES_WORDS: &str = include_str!("screen_words/es.json");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScreenWords {
    pub language: String,
    pub screen: String,
    pub words: BTreeMap<String, String>,
}

pub fn normalize_language(language: &str) -> Result<&'static str, String> {
    match language
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .as_str()
    {
        "en" | "en-us" | "english" => Ok("en"),
        "es" | "es-es" | "es-mx" | "spanish" | "espanol" => Ok("es"),
        _ => Err(format!(
            "OSL: no screen words file for language {language:?}"
        )),
    }
}

fn words_file(language: &str) -> Result<&'static str, String> {
    match normalize_language(language)? {
        "en" => Ok(EN_WORDS),
        "es" => Ok(ES_WORDS),
        other => Err(format!("OSL: no screen words file for language {other:?}")),
    }
}

pub fn load_screen_words(language: &str, screen: &str) -> Result<ScreenWords, String> {
    let language = normalize_language(language)?;
    let raw = words_file(language)?;
    let screens: BTreeMap<String, BTreeMap<String, String>> = serde_json::from_str(raw)
        .map_err(|e| format!("OSL: parse screen words file for {language}: {e}"))?;
    let screen = screen.trim();
    if screen.is_empty() {
        return Err("OSL: screen is empty".to_string());
    }
    let words = screens
        .get(screen)
        .cloned()
        .ok_or_else(|| format!("OSL: no words for screen {screen:?} in language {language}"))?;
    Ok(ScreenWords {
        language: language.to_string(),
        screen: screen.to_string(),
        words,
    })
}
