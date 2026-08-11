use osl_english_catalogue::EnglishCatalogue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
        _ => Err(format!(
            "OSL: English is the only registered interface catalogue; language {language:?} refused"
        )),
    }
}

pub fn load_screen_words(language: &str, screen: &str) -> Result<ScreenWords, String> {
    let language = normalize_language(language)?;
    let catalogue = EnglishCatalogue::packaged("ipc.screen_words").map_err(|e| e.to_string())?;
    let screen = screen.trim();
    if screen.is_empty() {
        return Err("OSL: screen is empty".to_string());
    }
    if screen != "welcome" {
        return Err(format!(
            "OSL: no words for screen {screen:?} in language {language}"
        ));
    }
    let words = [
        ("title", "welcome.title"),
        ("body", "welcome.body"),
        ("primary_button", "welcome.primary_button"),
    ]
    .into_iter()
    .map(|(field, key)| {
        catalogue
            .resolve(key, std::iter::empty::<(&str, &str)>())
            .map(|resolved| (field.to_owned(), resolved.value))
            .map_err(|error| error.to_string())
    })
    .collect::<Result<BTreeMap<_, _>, _>>()?;
    Ok(ScreenWords {
        language: language.to_string(),
        screen: screen.to_string(),
        words,
    })
}
