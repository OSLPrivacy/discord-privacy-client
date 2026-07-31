use serde_json::Value;
use std::collections::BTreeMap;

const F11_ANCHOR_DOC: &str =
    include_str!("../../../docs/plans/scrub-tree-merge-plan-f11-anchor.md");

fn extract_machine_record(markdown: &str) -> &str {
    let mut after_heading = false;
    let mut json_start = None;
    let mut offset = 0;

    for line_with_ending in markdown.split_inclusive('\n') {
        let line = line_with_ending.trim_end_matches(&['\r', '\n'][..]);
        let trimmed = line.trim();
        if !after_heading {
            after_heading = trimmed == "## Machine-readable anchor record";
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
            return &markdown[json_start.expect("json start recorded")..offset];
        }

        offset += line_with_ending.len();
    }

    panic!("f11 anchor record must include one machine-readable JSON block");
}

fn record() -> Value {
    serde_json::from_str(extract_machine_record(F11_ANCHOR_DOC))
        .expect("f11 anchor record must be valid JSON")
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .as_object()
        .and_then(|object| object.get(name))
        .unwrap_or_else(|| panic!("missing JSON field `{name}`"))
}

fn string_field<'a>(value: &'a Value, name: &str) -> &'a str {
    field(value, name)
        .as_str()
        .unwrap_or_else(|| panic!("JSON field `{name}` must be a string"))
}

fn bool_field(value: &Value, name: &str) -> bool {
    field(value, name)
        .as_bool()
        .unwrap_or_else(|| panic!("JSON field `{name}` must be a boolean"))
}

fn array_field<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    field(value, name)
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_else(|| panic!("JSON field `{name}` must be an array"))
}

fn assert_git_sha(label: &str, value: &str) {
    assert_eq!(value.len(), 40, "{label} must be a full git SHA-1");
    assert!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{label} must be hex encoded"
    );
}

#[test]
fn integration_branch_anchor_records_osl_newest_integration_403cfa2() {
    let record = record();

    assert_eq!(
        string_field(&record, "schema"),
        "osl-scrub-tree-merge-anchor-v1"
    );
    assert_eq!(string_field(&record, "unit"), "f11");
    assert_eq!(string_field(&record, "purpose"), "plan-integration-anchor");
    assert_eq!(string_field(&record, "branch"), "unit-f11");
    let worktree_path = string_field(&record, "worktree_path_template");
    assert!(
        worktree_path.strip_prefix("/home/<user>/").is_some(),
        "worktree path must identify the templated f11 worktree"
    );
    assert_eq!(worktree_path.rsplit('/').next(), Some("osl-unit-f11"));

    let created_from = field(&record, "created_from");
    assert_eq!(string_field(created_from, "line"), "main");
    assert_eq!(
        string_field(created_from, "sha"),
        "16778b297d3ec8d0358b7d3812a95f4f8443e462"
    );
    assert_eq!(
        string_field(created_from, "tree"),
        "e85e4bd48bfaab810845e307614ab8af238a33d3"
    );
    assert_git_sha("base commit", string_field(created_from, "sha"));
    assert_git_sha("base tree", string_field(created_from, "tree"));

    let integration_source = field(&record, "integration_source");
    let pinned_integration_sha = string_field(integration_source, "pinned_input_sha");
    let observed_integration_head = string_field(integration_source, "observed_head_sha");
    assert_eq!(
        string_field(integration_source, "line"),
        "osl-newest-integration"
    );
    assert_eq!(
        pinned_integration_sha,
        "403cfa2e090bf76ae4cb2950f3febcc72204fc59"
    );
    assert_ne!(
        pinned_integration_sha, observed_integration_head,
        "the pinned plan input must not be replaced by the newer integration HEAD"
    );
    assert_git_sha("pinned integration input", pinned_integration_sha);
    assert_git_sha("observed integration head", observed_integration_head);

    let merge_order: Vec<(&str, &str, &str)> = array_field(&record, "planned_merge_order")
        .iter()
        .map(|entry| {
            (
                string_field(entry, "line"),
                string_field(entry, "sha"),
                string_field(entry, "action"),
            )
        })
        .collect();
    assert_eq!(
        merge_order,
        vec![
            (
                "main",
                "16778b297d3ec8d0358b7d3812a95f4f8443e462",
                "branch-base"
            ),
            (
                "f1-footprint",
                "61933d3a4b50e410e3be1d5e05560d955ee72b4c",
                "merge-second"
            ),
            (
                "osl-newest-integration",
                pinned_integration_sha,
                "merge-last"
            ),
        ],
        "f11 must identify the branch anchor and keep the pinned integration input last"
    );

    let branch_creation = field(&record, "branch_creation");
    assert!(bool_field(branch_creation, "created_new_branch"));
    assert!(bool_field(branch_creation, "worktree_added"));
    assert!(
        !bool_field(branch_creation, "source_checkouts_mutated"),
        "branch creation must record that source checkouts were not mutated"
    );

    let checkout_records = array_field(&record, "source_checkout_head_verification");
    assert_eq!(
        checkout_records.len(),
        3,
        "the anchor must record exactly the three source checkout HEADs"
    );
    let checkout_heads: BTreeMap<&str, (&str, &str, &str)> = checkout_records
        .iter()
        .map(|entry| {
            (
                string_field(entry, "line"),
                (
                    string_field(entry, "path_template"),
                    string_field(entry, "before"),
                    string_field(entry, "after"),
                ),
            )
        })
        .collect();
    assert_eq!(
        checkout_heads.len(),
        3,
        "all three source checkouts must have an unchanged HEAD record"
    );

    for line in ["main", "f1-footprint", "osl-newest-integration"] {
        let (path_template, before, after) = checkout_heads
            .get(line)
            .copied()
            .unwrap_or_else(|| panic!("missing source checkout record for `{line}`"));
        assert!(
            path_template.strip_prefix("/home/<user>/").is_some(),
            "source checkout paths must stay templated"
        );
        assert_eq!(
            before, after,
            "source checkout `{line}` must record no HEAD mutation"
        );
        assert_git_sha(line, before);
    }

    let (_, integration_checkout_before, integration_checkout_after) = checkout_heads
        .get("osl-newest-integration")
        .copied()
        .expect("integration checkout record exists");
    assert_eq!(integration_checkout_before, observed_integration_head);
    assert_eq!(integration_checkout_after, observed_integration_head);
}
