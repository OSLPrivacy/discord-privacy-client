//! One runtime control surface for the four cover-compression layers.
//!
//! The earlier layer tasks deliberately landed independently.  That made it
//! easy for a sender to pick a convenient encoder, but it also made it too
//! easy to bake a particular combination into a call site.  This module is
//! the single policy boundary: every layer has the same three positions and
//! the encoder derives its word capacity from the supplied settings.
//!
//! `Low` is intentionally gentler than `High`.  For the visible layers it
//! uses an alternate-word schedule, so only every other cover word carries
//! the extra case/typo bit.  A reviewer can therefore dial a layer back
//! without changing the protected message it addresses.

use crate::{
    compute_shrunk_token_tag, compute_token_tag, misspell, SHRUNK_TOKEN_ID_BYTES,
    SHRUNK_TOKEN_TAG_BYTES, TOKEN_ID_BYTES, TOKEN_MAC_BYTES,
};
use serde::{Deserialize, Serialize};

/// How hard one cover layer is allowed to push its representation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LayerStrength {
    #[default]
    Off,
    Low,
    High,
}

impl LayerStrength {
    pub const ALL: [Self; 3] = [Self::Off, Self::Low, Self::High];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Low => "low",
            Self::High => "high",
        }
    }

    /// Parse the direct-command spelling of a layer setting.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" => Ok(Self::Off),
            "low" => Ok(Self::Low),
            "high" => Ok(Self::High),
            other => Err(format!(
                "OSL: unknown layer strength '{other}' (use off, low, or high)"
            )),
        }
    }
}

/// The only settings object used to choose a layered compact-cover encoding.
///
/// Nothing in the encoder assumes a layer is enabled: callers supply this
/// value for every cover.  Serde defaults keep a missing future preference
/// file conservative.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(default)]
pub struct CoverLayerSettings {
    pub pointer: LayerStrength,
    pub capitalisation: LayerStrength,
    pub vocabulary: LayerStrength,
    pub spelling: LayerStrength,
}

impl CoverLayerSettings {
    pub const fn new(
        pointer: LayerStrength,
        capitalisation: LayerStrength,
        vocabulary: LayerStrength,
        spelling: LayerStrength,
    ) -> Self {
        Self {
            pointer,
            capitalisation,
            vocabulary,
            spelling,
        }
    }

    /// Number of base-vocabulary bits carried by each cover word.
    pub const fn vocabulary_bits(self) -> usize {
        match self.vocabulary {
            LayerStrength::Off => 6,
            LayerStrength::Low => 7,
            LayerStrength::High => 8,
        }
    }

    const fn vocabulary_size(self) -> usize {
        1usize << self.vocabulary_bits()
    }
}

/// The two pointer representations selected by the first layer.
///
/// The unshrunk seed remains available for `Off` and `Low`; `High` uses the
/// shared-key handle introduced by the first shrinking layer.  Both variants
/// address the same private message at the caller's store boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayeredCoverInput {
    Seed([u8; TOKEN_ID_BYTES]),
    SharedHandle([u8; SHRUNK_TOKEN_ID_BYTES]),
}

/// The input does not match the configured pointer layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerControlError(pub String);

impl core::fmt::Display for LayerControlError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LayerControlError {}

/// Encode `input` using exactly the four supplied layer settings.
///
/// The receiver calls [`decode_layered_cover`] with the same saved settings.
/// This explicit setting is important: the written cover contains no marker
/// that would reveal how aggressively it was compressed.
pub fn encode_layered_cover(
    mac_key: &[u8],
    input: LayeredCoverInput,
    settings: CoverLayerSettings,
) -> Result<String, LayerControlError> {
    let bits = payload_bits(mac_key, input, settings)?;
    let mut cursor = 0usize;
    let mut words = Vec::new();
    let vocab_bits = settings.vocabulary_bits();
    while cursor < bits.len() {
        let position = words.len();
        let index = take_bits(&bits, &mut cursor, vocab_bits);
        let capital = if scheduled(settings.capitalisation, position) {
            take_bits(&bits, &mut cursor, 1) == 1
        } else {
            false
        };
        let misspelled = if scheduled(settings.spelling, position) {
            take_bits(&bits, &mut cursor, 1) == 1
        } else {
            false
        };
        let form = if misspelled {
            misspell::MISSPELL_FORMS / 2 + index
        } else {
            index
        };
        let word = misspell::form_word(form).expect("layer vocabulary index is in range");
        words.push(render_word(word, capital));
    }
    Ok(format!("{}.", words.join(" ")))
}

/// Decode a cover produced by [`encode_layered_cover`] under `settings`.
///
/// Exact canonical re-rendering rejects added, removed, reordered, or
/// differently-cased cover words before a caller receives a pointer.
pub fn decode_layered_cover(
    mac_key: &[u8],
    settings: CoverLayerSettings,
    cover: &str,
) -> Option<LayeredCoverInput> {
    let target_bits = payload_bit_count(settings);
    let tokens: Vec<&str> = cover.split_ascii_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let mut bits = Vec::with_capacity(target_bits);
    let mut token_cursor = 0usize;
    let vocab_bits = settings.vocabulary_bits();
    while bits.len() < target_bits {
        let token = *tokens.get(token_cursor)?;
        let (written, capital) = strip_word(token)?;
        let forms = misspell::parse_misspelled_forms(&written)?;
        if forms.len() != 1 {
            return None;
        }
        let form = forms[0];
        let position = token_cursor;
        let spelling_scheduled = scheduled(settings.spelling, position);
        let (index, misspelled) = if form < settings.vocabulary_size() {
            (form, false)
        } else if spelling_scheduled
            && (misspell::MISSPELL_FORMS / 2..misspell::MISSPELL_FORMS).contains(&form)
            && form - misspell::MISSPELL_FORMS / 2 < settings.vocabulary_size()
        {
            (form - misspell::MISSPELL_FORMS / 2, true)
        } else {
            return None;
        };
        if capital != scheduled(settings.capitalisation, position) && capital {
            return None;
        }
        push_bits(&mut bits, index, vocab_bits, target_bits);
        if scheduled(settings.capitalisation, position) {
            push_bits(&mut bits, capital as usize, 1, target_bits);
        }
        if spelling_scheduled {
            push_bits(&mut bits, misspelled as usize, 1, target_bits);
        }
        token_cursor += 1;
    }
    if token_cursor != tokens.len() {
        return None;
    }

    let input = input_from_bits(mac_key, settings, &bits)?;
    (encode_layered_cover(mac_key, input, settings).ok()? == cover).then_some(input)
}

fn scheduled(strength: LayerStrength, position: usize) -> bool {
    match strength {
        LayerStrength::Off => false,
        // A conservative setting changes only alternate words, leaving the
        // other half ordinary for a human readability review.
        LayerStrength::Low => position % 2 == 0,
        LayerStrength::High => true,
    }
}

fn payload_bit_count(settings: CoverLayerSettings) -> usize {
    match settings.pointer {
        LayerStrength::Off => (TOKEN_ID_BYTES + TOKEN_MAC_BYTES) * 8,
        // Low retains the original seed but shortens only its detector.  It
        // is a real, reversible middle position rather than an alias for off.
        LayerStrength::Low => (TOKEN_ID_BYTES + SHRUNK_TOKEN_TAG_BYTES) * 8,
        LayerStrength::High => (SHRUNK_TOKEN_ID_BYTES + SHRUNK_TOKEN_TAG_BYTES) * 8,
    }
}

fn payload_bits(
    mac_key: &[u8],
    input: LayeredCoverInput,
    settings: CoverLayerSettings,
) -> Result<Vec<bool>, LayerControlError> {
    let bytes: Vec<u8> = match (settings.pointer, input) {
        (LayerStrength::Off, LayeredCoverInput::Seed(seed)) => {
            let tag = compute_token_tag(mac_key, &seed);
            seed.into_iter().chain(tag).collect()
        }
        (LayerStrength::Low, LayeredCoverInput::Seed(seed)) => {
            let tag = compute_token_tag(mac_key, &seed);
            seed.into_iter()
                .chain(tag.into_iter().take(SHRUNK_TOKEN_TAG_BYTES))
                .collect()
        }
        (LayerStrength::High, LayeredCoverInput::SharedHandle(handle)) => {
            let tag = compute_shrunk_token_tag(mac_key, &handle);
            handle.into_iter().chain(tag).collect()
        }
        (LayerStrength::Off | LayerStrength::Low, LayeredCoverInput::SharedHandle(_)) => {
            return Err(LayerControlError(
                "OSL: pointer off/low requires the original seed".into(),
            ));
        }
        (LayerStrength::High, LayeredCoverInput::Seed(_)) => {
            return Err(LayerControlError(
                "OSL: pointer high requires the shared-key handle".into(),
            ));
        }
    };
    Ok(bytes_to_bits(&bytes))
}

fn input_from_bits(
    mac_key: &[u8],
    settings: CoverLayerSettings,
    bits: &[bool],
) -> Option<LayeredCoverInput> {
    let bytes = bits_to_bytes(bits)?;
    match settings.pointer {
        LayerStrength::Off => {
            let seed: [u8; TOKEN_ID_BYTES] = bytes.get(..TOKEN_ID_BYTES)?.try_into().ok()?;
            let expected = compute_token_tag(mac_key, &seed);
            (bytes.get(TOKEN_ID_BYTES..) == Some(expected.as_slice()))
                .then_some(LayeredCoverInput::Seed(seed))
        }
        LayerStrength::Low => {
            let seed: [u8; TOKEN_ID_BYTES] = bytes.get(..TOKEN_ID_BYTES)?.try_into().ok()?;
            let expected = compute_token_tag(mac_key, &seed);
            (bytes.get(TOKEN_ID_BYTES..) == Some(&expected[..SHRUNK_TOKEN_TAG_BYTES]))
                .then_some(LayeredCoverInput::Seed(seed))
        }
        LayerStrength::High => {
            let handle: [u8; SHRUNK_TOKEN_ID_BYTES] =
                bytes.get(..SHRUNK_TOKEN_ID_BYTES)?.try_into().ok()?;
            let expected = compute_shrunk_token_tag(mac_key, &handle);
            (bytes.get(SHRUNK_TOKEN_ID_BYTES..) == Some(expected.as_slice()))
                .then_some(LayeredCoverInput::SharedHandle(handle))
        }
    }
}

fn bytes_to_bits(bytes: &[u8]) -> Vec<bool> {
    bytes
        .iter()
        .flat_map(|byte| (0..8).rev().map(move |shift| byte >> shift & 1 == 1))
        .collect()
}

fn bits_to_bytes(bits: &[bool]) -> Option<Vec<u8>> {
    (bits.len() % 8 == 0).then(|| {
        bits.chunks(8)
            .map(|chunk| chunk.iter().fold(0u8, |value, bit| value << 1 | *bit as u8))
            .collect()
    })
}

fn take_bits(bits: &[bool], cursor: &mut usize, width: usize) -> usize {
    let mut value = 0usize;
    for _ in 0..width {
        value <<= 1;
        if let Some(bit) = bits.get(*cursor) {
            value |= *bit as usize;
            *cursor += 1;
        }
    }
    value
}

fn push_bits(bits: &mut Vec<bool>, value: usize, width: usize, target: usize) {
    for shift in (0..width).rev() {
        if bits.len() == target {
            break;
        }
        bits.push(value >> shift & 1 == 1);
    }
}

fn render_word(word: &str, capital: bool) -> String {
    if !capital {
        return word.to_owned();
    }
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut rendered = String::with_capacity(word.len());
    rendered.push(first.to_ascii_uppercase());
    rendered.push_str(chars.as_str());
    rendered
}

fn strip_word(token: &str) -> Option<(String, bool)> {
    let word = token.trim_matches(|c: char| {
        matches!(
            c,
            '.' | ',' | '!' | '?' | ';' | ':' | '"' | ')' | ']' | '(' | '['
        )
    });
    (!word.is_empty()).then(|| {
        let capital = word.chars().next().is_some_and(|c| c.is_ascii_uppercase());
        (word.to_owned(), capital)
    })
}
