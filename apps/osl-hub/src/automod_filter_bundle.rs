//! Signed AutoMod filter bundles evaluated only after a device has decrypted a
//! message.  The client has no policy branch for BASIC or STRICT: it accepts a
//! newer trusted signed bundle and evaluates the rules carried in that bundle.
//! In particular, Discord's fleet-trained SPAM trigger is intentionally absent;
//! a reported-plaintext model has no honest equivalent on this device.

use crypto::ed25519::{self, PublicKey, SecretKey, Signature};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const BUNDLE_DOMAIN: &[u8] = b"osl.automod.filter-bundle.v1\0";
const MAX_RULES: usize = 256;
const MAX_TOKEN_BYTES: usize = 128;

/// The names are metadata only.  Rule evaluation never branches on this enum.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FilterSetId {
    Basic,
    Strict,
}

impl FilterSetId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Basic => "BASIC",
            Self::Strict => "STRICT",
        }
    }
}

/// The only trigger types an already-decrypted local message can honestly run.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalRuleKind {
    LiteralKeyword { keyword: String },
    KeywordPattern { pattern: String },
    MentionCount { minimum: u32 },
}

impl LocalRuleKind {
    fn validate(&self) -> Result<(), BundleError> {
        match self {
            Self::LiteralKeyword { keyword } => validate_rule_text(keyword, "literal keyword"),
            Self::KeywordPattern { pattern } => validate_rule_text(pattern, "keyword pattern"),
            Self::MentionCount { minimum } if *minimum > 0 => Ok(()),
            Self::MentionCount { .. } => {
                Err(BundleError::Invalid("mention count must be positive"))
            }
        }
    }

    fn matches(&self, plaintext: &str) -> bool {
        match self {
            Self::LiteralKeyword { keyword } => contains_case_insensitive(plaintext, keyword),
            Self::KeywordPattern { pattern } => {
                wildcard_matches(&plaintext.to_lowercase(), &pattern.to_lowercase())
            }
            Self::MentionCount { minimum } => local_mention_count(plaintext) >= *minimum,
        }
    }
}

/// A stable rule identifier lets user interfaces describe the matched rule
/// without retaining plaintext.  The kind and its value are signed too.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRule {
    pub id: String,
    #[serde(flatten)]
    pub trigger: LocalRuleKind,
}

impl FilterRule {
    fn validate(&self) -> Result<(), BundleError> {
        validate_token(&self.id, "rule id")?;
        self.trigger.validate()
    }
}

/// Wire form for a downloaded filter set.  The detached signature covers every
/// other field, including the named set, issuer, monotonic version, and rules.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignedFilterBundle {
    pub id: FilterSetId,
    pub version: u64,
    pub issuer: String,
    pub rules: Vec<FilterRule>,
    pub signature: Option<Vec<u8>>,
}

impl SignedFilterBundle {
    pub fn unsigned(
        id: FilterSetId,
        version: u64,
        issuer: impl Into<String>,
        rules: Vec<FilterRule>,
    ) -> Self {
        Self {
            id,
            version,
            issuer: issuer.into(),
            rules,
            signature: None,
        }
    }

    /// Authoring-side helper.  Production clients only call [`FilterBundleClient::install_download`].
    pub fn sign(&mut self, signing_key: &SecretKey) -> Result<(), BundleError> {
        self.validate_unsigned()?;
        self.signature = Some(
            ed25519::sign(signing_key, &self.canonical_bytes())
                .as_bytes()
                .to_vec(),
        );
        Ok(())
    }

    pub fn encoded(&self) -> Result<Vec<u8>, BundleError> {
        serde_json::to_vec(self).map_err(|_| BundleError::Invalid("bundle serialization failed"))
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, BundleError> {
        let value: serde_json::Value = serde_json::from_slice(encoded)
            .map_err(|_| BundleError::Invalid("bundle JSON is invalid"))?;
        let rules = value
            .get("rules")
            .and_then(serde_json::Value::as_array)
            .ok_or(BundleError::Invalid("bundle rules are missing"))?;
        for rule in rules {
            let kind = rule
                .get("kind")
                .and_then(serde_json::Value::as_str)
                .ok_or(BundleError::Invalid("bundle rule kind is missing"))?;
            if !matches!(
                kind,
                "literal_keyword" | "keyword_pattern" | "mention_count"
            ) {
                return Err(BundleError::UnsupportedRuleKind(kind.to_owned()));
            }
        }
        serde_json::from_value(value).map_err(|_| BundleError::Invalid("bundle schema is invalid"))
    }

    fn validate_unsigned(&self) -> Result<(), BundleError> {
        if self.version == 0 {
            return Err(BundleError::Invalid("bundle version must be positive"));
        }
        validate_token(&self.issuer, "issuer")?;
        if self.rules.is_empty() || self.rules.len() > MAX_RULES {
            return Err(BundleError::Invalid("bundle rule count is invalid"));
        }
        let mut unique = BTreeSet::new();
        for rule in &self.rules {
            rule.validate()?;
            if !unique.insert(rule.clone()) {
                return Err(BundleError::Invalid("bundle rules must be unique"));
            }
        }
        Ok(())
    }

    fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = BUNDLE_DOMAIN.to_vec();
        push_text(&mut out, self.id.as_str());
        out.extend_from_slice(&self.version.to_be_bytes());
        push_text(&mut out, &self.issuer);
        out.extend_from_slice(&(self.rules.len() as u32).to_be_bytes());
        for rule in &self.rules {
            push_text(&mut out, &rule.id);
            match &rule.trigger {
                LocalRuleKind::LiteralKeyword { keyword } => {
                    out.push(1);
                    push_text(&mut out, keyword);
                }
                LocalRuleKind::KeywordPattern { pattern } => {
                    out.push(2);
                    push_text(&mut out, pattern);
                }
                LocalRuleKind::MentionCount { minimum } => {
                    out.push(3);
                    out.extend_from_slice(&minimum.to_be_bytes());
                }
            }
        }
        out
    }
}

/// Pinned issuer keys supplied with the client/release policy.  A claimed
/// issuer name is insufficient: it must resolve to one of these public keys.
#[derive(Clone, Default)]
pub struct TrustedIssuers(BTreeMap<String, PublicKey>);

impl TrustedIssuers {
    pub fn from_entries(entries: impl IntoIterator<Item = (String, PublicKey)>) -> Self {
        Self(entries.into_iter().collect())
    }

    fn key_for(&self, issuer: &str) -> Result<&PublicKey, BundleError> {
        self.0
            .get(issuer)
            .ok_or_else(|| BundleError::UnknownIssuer(issuer.to_owned()))
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum BundleError {
    Unsigned,
    UnknownIssuer(String),
    InvalidSignature,
    VersionNotNewer {
        id: FilterSetId,
        installed: u64,
        received: u64,
    },
    UnsupportedRuleKind(String),
    Invalid(&'static str),
    MissingInstalledBundle(FilterSetId),
}

impl std::fmt::Display for BundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsigned => write!(f, "AutoMod filter bundle is unsigned"),
            Self::UnknownIssuer(issuer) => write!(f, "AutoMod filter bundle issuer is unknown: {issuer}"),
            Self::InvalidSignature => write!(f, "AutoMod filter bundle signature is invalid"),
            Self::VersionNotNewer { id, installed, received } => write!(f, "AutoMod filter bundle {} version {received} is not newer than installed version {installed}", id.as_str()),
            Self::UnsupportedRuleKind(kind) => write!(f, "unsupported local AutoMod rule kind: {kind}"),
            Self::Invalid(reason) => write!(f, "AutoMod filter bundle is invalid: {reason}"),
            Self::MissingInstalledBundle(id) => write!(f, "AutoMod filter bundle {} is not installed", id.as_str()),
        }
    }
}

impl std::error::Error for BundleError {}

/// The local client.  Failed downloads are checked before mutation, so the
/// active filter set remains in force on every refusal path.
pub struct FilterBundleClient {
    trusted_issuers: TrustedIssuers,
    installed: BTreeMap<FilterSetId, SignedFilterBundle>,
}

impl FilterBundleClient {
    pub fn new(trusted_issuers: TrustedIssuers) -> Self {
        Self {
            trusted_issuers,
            installed: BTreeMap::new(),
        }
    }

    pub fn install_download(&mut self, encoded: &[u8]) -> Result<(), BundleError> {
        let bundle = SignedFilterBundle::decode(encoded)?;
        bundle.validate_unsigned()?;
        let signature = bundle.signature.as_ref().ok_or(BundleError::Unsigned)?;
        let bytes: [u8; ed25519::SIGNATURE_SIZE] = signature
            .as_slice()
            .try_into()
            .map_err(|_| BundleError::InvalidSignature)?;
        let key = self.trusted_issuers.key_for(&bundle.issuer)?;
        if !ed25519::verify(
            key,
            &bundle.canonical_bytes(),
            &Signature::from_bytes(bytes),
        )
        .map_err(|_| BundleError::InvalidSignature)?
        {
            return Err(BundleError::InvalidSignature);
        }
        if let Some(installed) = self.installed.get(&bundle.id) {
            if bundle.version <= installed.version {
                return Err(BundleError::VersionNotNewer {
                    id: bundle.id,
                    installed: installed.version,
                    received: bundle.version,
                });
            }
        }
        self.installed.insert(bundle.id, bundle);
        Ok(())
    }

    pub fn installed(&self, id: FilterSetId) -> Option<&SignedFilterBundle> {
        self.installed.get(&id)
    }

    pub fn evaluate(
        &self,
        id: FilterSetId,
        decrypted_plaintext: &str,
    ) -> Result<Vec<String>, BundleError> {
        let bundle = self
            .installed
            .get(&id)
            .ok_or(BundleError::MissingInstalledBundle(id))?;
        Ok(bundle
            .rules
            .iter()
            .filter(|rule| rule.trigger.matches(decrypted_plaintext))
            .map(|rule| rule.id.clone())
            .collect())
    }
}

/// Authoring content for the current BASIC policy.  Once signed, these are data
/// in a downloaded bundle; the local evaluator does not special-case them.
pub fn basic_rules() -> Vec<FilterRule> {
    [
        (
            "slur-faggot",
            LocalRuleKind::LiteralKeyword {
                keyword: "faggot".into(),
            },
        ),
        (
            "slur-kike",
            LocalRuleKind::LiteralKeyword {
                keyword: "kike".into(),
            },
        ),
        (
            "slur-spic",
            LocalRuleKind::LiteralKeyword {
                keyword: "spic".into(),
            },
        ),
        (
            "slur-retard",
            LocalRuleKind::LiteralKeyword {
                keyword: "retard".into(),
            },
        ),
        (
            "sexual-porn",
            LocalRuleKind::LiteralKeyword {
                keyword: "porn".into(),
            },
        ),
        (
            "sexual-nudes",
            LocalRuleKind::LiteralKeyword {
                keyword: "nudes".into(),
            },
        ),
        (
            "sexual-onlyfans",
            LocalRuleKind::KeywordPattern {
                pattern: "*onlyfans*".into(),
            },
        ),
    ]
    .into_iter()
    .map(|(id, trigger)| FilterRule {
        id: id.into(),
        trigger,
    })
    .collect()
}

/// STRICT is deliberately built by adding locally evaluatable policy data to
/// BASIC, making its set relationship auditable rather than label-dependent.
pub fn strict_rules() -> Vec<FilterRule> {
    let mut rules = basic_rules();
    rules.extend([
        FilterRule {
            id: "profanity-damn".into(),
            trigger: LocalRuleKind::LiteralKeyword {
                keyword: "damn".into(),
            },
        },
        FilterRule {
            id: "profanity-shit".into(),
            trigger: LocalRuleKind::LiteralKeyword {
                keyword: "shit".into(),
            },
        },
        FilterRule {
            id: "profanity-fuck".into(),
            trigger: LocalRuleKind::LiteralKeyword {
                keyword: "fuck".into(),
            },
        },
        FilterRule {
            id: "mention-count-3".into(),
            trigger: LocalRuleKind::MentionCount { minimum: 3 },
        },
    ]);
    rules
}

fn validate_token(value: &str, field: &'static str) -> Result<(), BundleError> {
    if value.is_empty()
        || value.len() > MAX_TOKEN_BYTES
        || value
            .bytes()
            .any(|byte| !matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
    {
        return Err(BundleError::Invalid(field));
    }
    Ok(())
}

fn validate_rule_text(value: &str, field: &'static str) -> Result<(), BundleError> {
    if value.is_empty() || value.len() > MAX_TOKEN_BYTES || value.chars().any(char::is_control) {
        return Err(BundleError::Invalid(field));
    }
    Ok(())
}

fn push_text(out: &mut Vec<u8>, value: &str) {
    out.extend_from_slice(&(value.len() as u32).to_be_bytes());
    out.extend_from_slice(value.as_bytes());
}

fn contains_case_insensitive(text: &str, needle: &str) -> bool {
    text.to_lowercase().contains(&needle.to_lowercase())
}

/// Bounded glob matching: `*` is any sequence and `?` one Unicode scalar.
/// Unlike an unbounded regex engine, this remains a straightforward device-only
/// predicate for keyword patterns delivered in a signed bundle.
fn wildcard_matches(text: &str, pattern: &str) -> bool {
    let text = text.chars().collect::<Vec<_>>();
    let pattern = pattern.chars().collect::<Vec<_>>();
    let (mut text_at, mut pattern_at, mut star, mut retry) = (0usize, 0usize, None, 0usize);
    while text_at < text.len() {
        if pattern
            .get(pattern_at)
            .is_some_and(|item| *item == '?' || *item == text[text_at])
        {
            text_at += 1;
            pattern_at += 1;
        } else if pattern.get(pattern_at) == Some(&'*') {
            star = Some(pattern_at);
            pattern_at += 1;
            retry = text_at;
        } else if let Some(star_at) = star {
            pattern_at = star_at + 1;
            retry += 1;
            text_at = retry;
        } else {
            return false;
        }
    }
    while pattern.get(pattern_at) == Some(&'*') {
        pattern_at += 1;
    }
    pattern_at == pattern.len()
}

fn local_mention_count(text: &str) -> u32 {
    let chars = text.chars().collect::<Vec<_>>();
    chars
        .iter()
        .enumerate()
        .filter(|(index, character)| {
            **character == '@'
                && chars
                    .get(index + 1)
                    .is_some_and(|next| next.is_alphanumeric() || *next == '_')
                && (*index == 0 || !chars[index - 1].is_alphanumeric())
        })
        .count() as u32
}
