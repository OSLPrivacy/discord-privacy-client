use serde::Deserialize;
use std::collections::BTreeSet;

const DESIGN_FEEL_DOC: &str = include_str!("../../../docs/design/osl-subjective-design-feel.md");
const TEST_TO_CREATE: &str = "docs/design/osl-subjective-design-feel.md";

#[derive(Debug, Deserialize)]
struct ContractFixtures {
    schema: String,
    unit: String,
    contract: String,
    allowed_product_nouns: Vec<String>,
    banned_user_facing_concepts: Vec<String>,
    cases: Vec<SurfaceCase>,
}

#[derive(Debug, Deserialize)]
struct SurfaceCase {
    name: String,
    surface: String,
    #[serde(default)]
    visible_choices: Vec<String>,
    #[serde(default)]
    main_screen_answer: Option<String>,
    #[serde(default)]
    refusal: Option<Refusal>,
    #[serde(default)]
    support_export: Option<SupportExport>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct Refusal {
    consequence: String,
    safe_action: String,
}

#[derive(Debug, Deserialize)]
struct SupportExport {
    machine_fields_secondary: bool,
    fields: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Expected {
    Pass,
    Fail,
}

// The returned slice borrows from `markdown` only, so tie the lifetime to that
// rather than to both parameters.
fn extract_json_block<'a>(markdown: &'a str, heading: &str) -> &'a str {
    let mut after_heading = false;
    let mut json_start = None;
    let mut offset = 0;

    for line_with_ending in markdown.split_inclusive('\n') {
        let line = line_with_ending.trim_end_matches(&['\r', '\n'][..]);
        let trimmed = line.trim();
        if !after_heading {
            after_heading = trimmed == heading;
            offset += line_with_ending.len();
            continue;
        }

        if json_start.is_none() {
            if trimmed == "```json" {
                json_start = Some(offset + line_with_ending.len());
            }
            offset += line_with_ending.len();
            continue;
        }

        if trimmed == "```" {
            return &markdown[json_start.expect("JSON start recorded")..offset];
        }

        offset += line_with_ending.len();
    }

    panic!("design-feel contract must include a machine-readable JSON fixture block");
}

fn fixtures() -> ContractFixtures {
    serde_json::from_str(extract_json_block(
        DESIGN_FEEL_DOC,
        "## Machine-readable product-contract acceptance fixtures",
    ))
    .expect("design-feel contract fixtures must be valid JSON")
}

fn lower_set(values: &[String]) -> BTreeSet<String> {
    values.iter().map(|value| value.to_lowercase()).collect()
}

/// Banned concepts are declared in the plural product form the contract names
/// them by ("receipts", "provider adapters", "automation internals"). A surface
/// exposes exactly the same implementation machinery when it names one of them
/// in the singular, so comparing against the plural literal alone is a hole in
/// the gate that covers every entry in the list rather than one fixture: it let
/// "Ratchet receipt is missing for this provider adapter" through as if it were
/// product language. Match the singular stem so both forms are caught.
fn banned_concept_stem(concept: &str) -> &str {
    concept.strip_suffix('s').unwrap_or(concept)
}

fn has_banned_language(text: &str, banned_concepts: &BTreeSet<String>) -> bool {
    let normalized = text.to_lowercase();
    banned_concepts
        .iter()
        .any(|concept| normalized.contains(banned_concept_stem(concept)))
}

fn case_preserves_product_contract(
    case: &SurfaceCase,
    product_nouns: &BTreeSet<String>,
    banned_concepts: &BTreeSet<String>,
) -> bool {
    if case.surface.trim().is_empty() {
        return false;
    }
    if case.visible_choices.is_empty()
        && case.main_screen_answer.is_none()
        && case.refusal.is_none()
        && case.support_export.is_none()
    {
        return false;
    }

    if !case.visible_choices.is_empty()
        && !case
            .visible_choices
            .iter()
            .all(|choice| product_nouns.contains(&choice.to_lowercase()))
    {
        return false;
    }

    if let Some(answer) = &case.main_screen_answer {
        if answer.trim().is_empty() || has_banned_language(answer, banned_concepts) {
            return false;
        }
    }

    if let Some(refusal) = &case.refusal {
        let consequence = refusal.consequence.trim();
        let safe_action = refusal.safe_action.trim();
        if consequence.is_empty() || safe_action.is_empty() {
            return false;
        }
        if has_banned_language(consequence, banned_concepts)
            || has_banned_language(safe_action, banned_concepts)
        {
            return false;
        }
    }

    if let Some(export) = &case.support_export {
        if !export.machine_fields_secondary {
            return false;
        }
        if export.fields.is_empty() || case.main_screen_answer.is_none() {
            return false;
        }
    }

    true
}

#[test]
fn freeze_the_user_facing_complexity_hiding_product_contract() {
    assert_eq!(TEST_TO_CREATE, "docs/design/osl-subjective-design-feel.md");

    let fixtures = fixtures();
    assert_eq!(fixtures.schema, "osl-subjective-design-feel-contract-v1");
    assert_eq!(fixtures.unit, "j1");
    assert_eq!(fixtures.contract, "complexity-hiding-product-model");

    let product_nouns = lower_set(&fixtures.allowed_product_nouns);
    let banned_concepts = lower_set(&fixtures.banned_user_facing_concepts);
    assert_eq!(
        product_nouns,
        BTreeSet::from([
            "protection state".to_string(),
            "trusted people".to_string(),
            "connected accounts".to_string(),
            "private conversations".to_string(),
            "cleanup actions".to_string(),
            "activity history".to_string(),
        ])
    );
    assert!(
        banned_concepts.is_superset(&BTreeSet::from([
            "keyservers".to_string(),
            "ratchets".to_string(),
            "receipts".to_string(),
            "browser profiles".to_string(),
            "provider adapters".to_string(),
        ])),
        "the contract must keep implementation machinery out of user-facing surfaces"
    );

    let mut saw_pass = false;
    let mut saw_fail = false;
    for case in &fixtures.cases {
        let actual = case_preserves_product_contract(case, &product_nouns, &banned_concepts);
        match case.expected {
            Expected::Pass => {
                saw_pass = true;
                assert!(actual, "{} should preserve the product contract", case.name);
            }
            Expected::Fail => {
                saw_fail = true;
                assert!(
                    !actual,
                    "{} should be rejected for exposing implementation machinery",
                    case.name
                );
            }
        }
    }
    assert!(
        saw_pass,
        "the contract needs at least one accepted product surface"
    );
    assert!(
        saw_fail,
        "the contract needs at least one inverted failing surface"
    );
}
